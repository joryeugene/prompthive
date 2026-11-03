//! Checked, same-directory writes shared by prompt mutations and explicit exports.

#[cfg(unix)]
use anyhow::anyhow;
use anyhow::{bail, Context, Result};
use std::fmt;
use std::fs::{self, File, Metadata};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub struct ConflictError {
    pub path: PathBuf,
    pub recovery: Option<PathBuf>,
}

impl fmt::Display for ConflictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "edit conflict: {} changed since it was read",
            self.path.display()
        )?;
        if let Some(path) = &self.recovery {
            write!(
                f,
                "; your proposed content is preserved at {}",
                path.display()
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for ConflictError {}

pub(crate) struct Directory {
    file: File,
    path: PathBuf,
}

impl Directory {
    /// Walk from the filesystem root without following directory symlinks.
    pub(crate) fn open(path: &Path, create: bool) -> Result<Self> {
        if !path.is_absolute() {
            bail!("directory must be absolute: {}", path.display());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open("/")?;
            let mut directory = Self {
                file,
                path: PathBuf::from("/"),
            };
            for part in path.components() {
                match part {
                    Component::RootDir => {}
                    Component::Normal(name) => directory = directory.child(name, create)?,
                    _ => bail!("invalid directory component: {}", path.display()),
                }
            }
            Ok(directory)
        }
        #[cfg(not(unix))]
        {
            let mut current = PathBuf::new();
            for part in path.components() {
                current.push(part.as_os_str());
                // A Windows drive/verbatim prefix is not a complete path until
                // its root separator arrives. Never stat a partial \?\C: prefix.
                if matches!(part, Component::Prefix(_)) {
                    continue;
                }
                match fs::symlink_metadata(&current) {
                    Ok(meta) if is_link(&meta) => {
                        bail!("symlink directory is not allowed: {}", current.display())
                    }
                    Ok(meta) if !meta.is_dir() => bail!("not a directory: {}", current.display()),
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound && create => {
                        fs::create_dir(&current)?
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            #[cfg(windows)]
            let file = {
                use std::os::windows::fs::OpenOptionsExt;
                fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(0x02000000)
                    .open(path)?
            };
            #[cfg(not(windows))]
            let file = File::open(path)?;
            Ok(Self {
                file,
                path: path.to_path_buf(),
            })
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn child(&self, name: &std::ffi::OsStr, create: bool) -> Result<Self> {
        check_leaf(name)?;
        let path = self.path.join(name);
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let name = c_name(name)?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            let mut fd = unsafe { libc::openat(self.file.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 && io::Error::last_os_error().kind() == io::ErrorKind::NotFound && create {
                let result = unsafe { libc::mkdirat(self.file.as_raw_fd(), name.as_ptr(), 0o700) };
                if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
                    return Err(io::Error::last_os_error())
                        .with_context(|| format!("cannot create directory {}", path.display()));
                }
                self.sync()?;
                fd = unsafe { libc::openat(self.file.as_raw_fd(), name.as_ptr(), flags) };
            }
            if fd < 0 {
                return Err(io::Error::last_os_error()).with_context(|| {
                    format!(
                        "cannot open safe directory {} (symlinks are not followed)",
                        path.display()
                    )
                });
            }
            Ok(Self {
                file: unsafe { File::from_raw_fd(fd) },
                path,
            })
        }
        #[cfg(not(unix))]
        {
            Self::open(&path, create)
        }
    }

    /// Create exactly one directory, distinguishing a new store from an
    /// existing empty one without a check-then-create race.
    pub(crate) fn create_child(&self, name: &std::ffi::OsStr) -> Result<Option<Self>> {
        check_leaf(name)?;
        #[cfg(unix)]
        let created = {
            use std::os::fd::AsRawFd;
            let name = c_name(name)?;
            if unsafe { libc::mkdirat(self.file.as_raw_fd(), name.as_ptr(), 0o700) } == 0 {
                true
            } else {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::AlreadyExists {
                    return Err(error.into());
                }
                false
            }
        };
        #[cfg(not(unix))]
        let created = match fs::create_dir(self.path.join(name)) {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => false,
            Err(error) => return Err(error.into()),
        };
        let directory = self.child(name, false)?;
        if created {
            self.sync()?;
            Ok(Some(directory))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn descend(&self, relative: &Path, create: bool) -> Result<Self> {
        let mut directory = Self {
            file: self.file.try_clone()?,
            path: self.path.clone(),
        };
        for part in relative.components() {
            match part {
                Component::Normal(name) => directory = directory.child(name, create)?,
                _ => bail!("unsafe relative directory: {}", relative.display()),
            }
        }
        Ok(directory)
    }

    pub(crate) fn entries(&self) -> Result<Vec<std::ffi::OsString>> {
        let mut budget =
            crate::limits::Budget::new(crate::limits::IMPORT_BYTES, "directory listing");
        self.entries_budgeted(&mut budget)
    }

    pub(crate) fn entries_budgeted(
        &self,
        budget: &mut crate::limits::Budget,
    ) -> Result<Vec<std::ffi::OsString>> {
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "freebsd"
        ))]
        {
            use std::os::fd::AsRawFd;
            use std::os::unix::ffi::OsStringExt;
            // dup shares a directory cursor; openat gives each listing its own
            // cursor while remaining anchored to this checked directory handle.
            let fd = unsafe {
                libc::openat(
                    self.file.as_raw_fd(),
                    c".".as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error())
                    .context("cannot duplicate directory handle");
            }
            let pointer = unsafe { libc::fdopendir(fd) };
            if pointer.is_null() {
                unsafe {
                    libc::close(fd);
                }
                return Err(io::Error::last_os_error()).context("cannot list directory");
            }
            struct Listing(*mut libc::DIR);
            impl Drop for Listing {
                fn drop(&mut self) {
                    unsafe {
                        libc::closedir(self.0);
                    }
                }
            }
            let listing = Listing(pointer);
            let mut result = Vec::new();
            loop {
                #[cfg(any(target_os = "linux", target_os = "android"))]
                let errno = unsafe { libc::__errno_location() };
                #[cfg(any(target_os = "macos", target_os = "freebsd"))]
                let errno = unsafe { libc::__error() };
                unsafe {
                    *errno = 0;
                }
                let entry = unsafe { libc::readdir(listing.0) };
                if entry.is_null() {
                    if unsafe { *errno } != 0 {
                        return Err(io::Error::last_os_error())
                            .context("cannot read directory entries");
                    }
                    break;
                }
                let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if name != b"." && name != b".." {
                    budget.add(name.len())?;
                    budget.add(2 * std::mem::size_of::<std::ffi::OsString>())?;
                    result.push(std::ffi::OsString::from_vec(name.to_vec()));
                }
            }
            result.sort();
            Ok(result)
        }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "freebsd"
        )))]
        {
            let mut result = Vec::new();
            for entry in fs::read_dir(&self.path)? {
                let name = entry?.file_name();
                budget.add(name.as_encoded_bytes().len())?;
                budget.add(2 * std::mem::size_of::<std::ffi::OsString>())?;
                result.push(name);
            }
            result.sort();
            Ok(result)
        }
    }

    fn open_entry(&self, name: &std::ffi::OsStr) -> Result<File> {
        check_leaf(name)?;
        let path = self.path.join(name);
        #[cfg(unix)]
        let file = {
            use std::os::fd::{AsRawFd, FromRawFd};
            let name = c_name(name)?;
            let fd = unsafe {
                libc::openat(
                    self.file.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error()).with_context(|| {
                    format!("cannot read {} (symlinks are not followed)", path.display())
                });
            }
            unsafe { File::from_raw_fd(fd) }
        };
        #[cfg(not(unix))]
        let file = {
            if is_link(&fs::symlink_metadata(&path)?) {
                bail!("symlink file is not allowed: {}", path.display());
            }
            File::open(&path)?
        };
        Ok(file)
    }

    pub(crate) fn metadata(&self, name: &std::ffi::OsStr) -> Result<Metadata> {
        #[cfg(unix)]
        {
            Ok(self.open_entry(name)?.metadata()?)
        }
        #[cfg(not(unix))]
        {
            check_leaf(name)?;
            let path = self.path.join(name);
            let metadata = fs::symlink_metadata(&path)?;
            if is_link(&metadata) {
                bail!("linked entry is not allowed: {}", path.display());
            }
            Ok(metadata)
        }
    }

    pub(crate) fn open_file(&self, name: &std::ffi::OsStr) -> Result<File> {
        let file = self.open_entry(name)?;
        if !file.metadata()?.is_file() {
            bail!("not a regular file: {}", self.path.join(name).display());
        }
        Ok(file)
    }

    pub(crate) fn read(&self, name: &std::ffi::OsStr) -> Result<(Vec<u8>, Metadata)> {
        let file = self.open_file(name)?;
        let meta = file.metadata()?;
        let label = format!("file {}", self.path.join(name).display());
        if meta.len() > crate::limits::TEXT_BYTES as u64 {
            return Err(crate::limits::exceeded(&label, crate::limits::TEXT_BYTES));
        }
        let bytes = crate::limits::read(file, crate::limits::TEXT_BYTES, &label)?;
        Ok((bytes, meta))
    }

    fn read_optional(&self, name: &std::ffi::OsStr) -> Result<Option<(Vec<u8>, Metadata)>> {
        match self.read(name) {
            Ok(value) => Ok(Some(value)),
            Err(error)
                if error.chain().any(|cause| {
                    cause
                        .downcast_ref::<io::Error>()
                        .is_some_and(|e| e.kind() == io::ErrorKind::NotFound)
                }) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn lock_file(&self) -> Result<File> {
        let name = std::ffi::OsStr::new(".write.lock");
        #[cfg(unix)]
        let file = {
            use std::os::fd::{AsRawFd, FromRawFd};
            let name = c_name(name)?;
            let fd = unsafe {
                libc::openat(
                    self.file.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error()).context("cannot open store write lock");
            }
            unsafe { File::from_raw_fd(fd) }
        };
        #[cfg(not(unix))]
        let file = {
            let path = self.path.join(name);
            if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                bail!("symlink write lock is not allowed");
            }
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?
        };
        if !file.metadata()?.is_file() {
            bail!("store write lock is not a regular file");
        }
        file.lock().context("cannot acquire store write lock")?;
        Ok(file)
    }

    pub(crate) fn remove(&self, name: &std::ffi::OsStr) -> Result<()> {
        if self.open_file(name)?.metadata()?.permissions().readonly() {
            bail!(
                "permission denied: {} is read-only",
                self.path.join(name).display()
            );
        }
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let name = c_name(name)?;
            if unsafe { libc::unlinkat(self.file.as_raw_fd(), name.as_ptr(), 0) } < 0 {
                return Err(io::Error::last_os_error()).context("cannot remove prompt");
            }
        }
        #[cfg(not(unix))]
        fs::remove_file(self.path.join(name))?;
        self.sync()
            .context("removal completed, but directory durability is uncertain")
    }

    pub(crate) fn sync(&self) -> Result<()> {
        #[cfg(unix)]
        self.file
            .sync_all()
            .with_context(|| format!("cannot sync directory {}", self.path.display()))?;
        // Rust has no portable directory-sync operation on Windows. File data is
        // synced; power-loss directory durability is not claimed there.
        Ok(())
    }

    pub(crate) fn write_atomic(
        &self,
        name: &std::ffi::OsStr,
        bytes: &[u8],
        overwrite: bool,
        expected: Option<&[u8]>,
    ) -> Result<()> {
        check_leaf(name)?;
        let path = self.path.join(name);
        let old = self.read_optional(name)?;
        if old.is_some() && !overwrite {
            bail!("file already exists: {}", path.display());
        }
        if old
            .as_ref()
            .is_some_and(|(_, meta)| meta.permissions().readonly())
        {
            bail!("permission denied: {} is read-only", path.display());
        }
        if let Some(expected) = expected {
            if old.as_ref().map(|(bytes, _)| bytes.as_slice()) != Some(expected) {
                return Err(ConflictError {
                    path,
                    recovery: None,
                }
                .into());
            }
        }
        fail(Stage::BeforeWrite)?;
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};
            let temporary_name = format!(".ph-{}.tmp", uuid::Uuid::new_v4());
            let temporary_c = c_name(std::ffi::OsStr::new(&temporary_name))?;
            let fd = unsafe {
                libc::openat(
                    self.file.as_raw_fd(),
                    temporary_c.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if fd < 0 {
                return Err(io::Error::last_os_error()).context("cannot create temporary file");
            }
            struct Temporary<'a> {
                directory: &'a File,
                name: std::ffi::CString,
            }
            impl Drop for Temporary<'_> {
                fn drop(&mut self) {
                    use std::os::fd::AsRawFd;
                    unsafe {
                        libc::unlinkat(self.directory.as_raw_fd(), self.name.as_ptr(), 0);
                    }
                }
            }
            let _temporary = Temporary {
                directory: &self.file,
                name: temporary_c,
            };
            let mut file = unsafe { File::from_raw_fd(fd) };
            write_staged(&mut file, bytes)
                .with_context(|| format!("cannot stage {}", path.display()))?;
            file.flush()?;
            fail(Stage::AfterWrite)?;
            if let Some((_, meta)) = &old {
                file.set_permissions(meta.permissions())?;
            }
            file.sync_all().context("cannot sync staged file")?;
            ensure_unchanged(&path, &old, &self.read_optional(name)?)?;
            fail(Stage::BeforeReplace)?;
            let destination = c_name(name)?;
            let result = unsafe {
                if overwrite && old.is_some() {
                    libc::renameat(
                        self.file.as_raw_fd(),
                        _temporary.name.as_ptr(),
                        self.file.as_raw_fd(),
                        destination.as_ptr(),
                    )
                } else {
                    libc::linkat(
                        self.file.as_raw_fd(),
                        _temporary.name.as_ptr(),
                        self.file.as_raw_fd(),
                        destination.as_ptr(),
                        0,
                    )
                }
            };
            if result < 0 {
                return Err(io::Error::last_os_error())
                    .with_context(|| format!("cannot commit {}", path.display()));
            }
            if (!overwrite || old.is_none())
                && unsafe { libc::unlinkat(self.file.as_raw_fd(), _temporary.name.as_ptr(), 0) } < 0
            {
                return Err(io::Error::last_os_error())
                    .context("new file exists, but temporary-file cleanup failed");
            }
            fail(Stage::AfterReplace).context("replacement completed; durability is uncertain")?;
            self.sync()
                .context("replacement completed; durability is uncertain")
        }
        #[cfg(not(unix))]
        {
            let mut temporary = tempfile::NamedTempFile::new_in(&self.path)?;
            write_staged(temporary.as_file_mut(), bytes)?;
            temporary.flush()?;
            fail(Stage::AfterWrite)?;
            if let Some((_, meta)) = &old {
                temporary.as_file().set_permissions(meta.permissions())?;
            }
            temporary.as_file().sync_all()?;
            ensure_unchanged(&path, &old, &self.read_optional(name)?)?;
            fail(Stage::BeforeReplace)?;
            if overwrite && old.is_some() {
                temporary.persist(&path).map_err(|e| e.error)?;
            } else {
                temporary.persist_noclobber(&path).map_err(|e| e.error)?;
            }
            fail(Stage::AfterReplace).context("replacement completed; durability is uncertain")?;
            self.sync()
                .context("replacement completed; durability is uncertain")
        }
    }
}

#[cfg(not(unix))]
fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Reject junctions and other reparse points as well as symbolic links.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn ensure_unchanged(
    path: &Path,
    old: &Option<(Vec<u8>, Metadata)>,
    current: &Option<(Vec<u8>, Metadata)>,
) -> Result<()> {
    let old_value = old
        .as_ref()
        .map(|(bytes, meta)| (bytes, meta.permissions()));
    let new_value = current
        .as_ref()
        .map(|(bytes, meta)| (bytes, meta.permissions()));
    if old_value != new_value {
        return Err(ConflictError {
            path: path.to_path_buf(),
            recovery: None,
        }
        .into());
    }
    Ok(())
}

fn check_leaf(name: &std::ffi::OsStr) -> Result<()> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        bail!("unsafe filename: {}", name.to_string_lossy());
    }
    Ok(())
}

#[cfg(unix)]
fn c_name(name: &std::ffi::OsStr) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(name.as_bytes()).map_err(|_| anyhow!("filename contains NUL"))
}

/// Atomic explicit-file output. Existing files require overwrite; backup is
/// mandatory before replacement when requested by the caller.
pub fn atomic_write(path: &Path, bytes: &[u8], overwrite: bool, backup: bool) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent)
        .with_context(|| format!("cannot open output directory {}", parent.display()))?;
    let name = path.file_name().context("output path has no filename")?;
    check_leaf(name)?;
    let directory = Directory::open(&parent, false)?;
    let _lock = directory.lock_file()?;
    let original = directory.read_optional(name)?;
    if original.is_some() && !overwrite {
        bail!(
            "file already exists: {}; use --force to replace it",
            path.display()
        );
    }
    if original
        .as_ref()
        .is_some_and(|(_, meta)| meta.permissions().readonly())
    {
        bail!("permission denied: {} is read-only", path.display());
    }
    if backup {
        if let Some((old, _)) = &original {
            let backups = directory
                .child(std::ffi::OsStr::new(".ph-backups"), true)?
                .child(name, true)?;
            let label = format!("{}.bak", uuid::Uuid::new_v4());
            backups
                .write_atomic(std::ffi::OsStr::new(&label), old, false, None)
                .context("cannot create required output backup; original file is unchanged")?;
        }
    }
    directory.write_atomic(
        name,
        bytes,
        overwrite,
        original.as_ref().map(|(bytes, _)| bytes.as_slice()),
    )
}

pub fn sync_directory(path: &Path) -> Result<()> {
    Directory::open(path, false)?.sync()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    BeforeWrite,
    #[cfg(test)]
    DuringWrite,
    AfterWrite,
    BeforeReplace,
    AfterReplace,
}
#[cfg(test)]
thread_local! { static FAIL_AT: std::cell::Cell<Option<Stage>> = const { std::cell::Cell::new(None) }; }
fn write_staged(file: &mut File, bytes: &[u8]) -> Result<()> {
    #[cfg(test)]
    if FAIL_AT.with(|value| value.get()) == Some(Stage::DuringWrite) {
        file.write_all(&bytes[..bytes.len() / 2])?;
        return Err(io::Error::new(
            io::ErrorKind::StorageFull,
            "injected disk full during staged write",
        )
        .into());
    }
    file.write_all(bytes)?;
    Ok(())
}

fn fail(_stage: Stage) -> Result<()> {
    #[cfg(test)]
    if FAIL_AT.with(|value| value.get()) == Some(_stage) {
        bail!("injected write failure");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_listing_checks_each_name_before_vector_growth() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::write(root.join("a"), b"a").unwrap();
        fs::write(root.join("b"), b"b").unwrap();
        let directory = Directory::open(&root, false).unwrap();
        let exact = 2 * (1 + 2 * std::mem::size_of::<std::ffi::OsString>());
        let mut budget = crate::limits::Budget::new(exact, "fixture directory");
        assert_eq!(
            directory.entries_budgeted(&mut budget).unwrap(),
            vec![std::ffi::OsString::from("a"), std::ffi::OsString::from("b")]
        );
        let mut budget = crate::limits::Budget::new(exact - 1, "fixture directory");
        assert!(directory
            .entries_budgeted(&mut budget)
            .unwrap_err()
            .downcast_ref::<crate::limits::LimitError>()
            .is_some());
    }
    fn fail_at(stage: Stage, action: impl FnOnce()) {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                FAIL_AT.with(|value| value.set(None));
            }
        }
        FAIL_AT.with(|value| value.set(Some(stage)));
        let _reset = Reset;
        action();
    }
    #[test]
    fn write_failures_before_commit_keep_the_original_whole() {
        for stage in [
            Stage::BeforeWrite,
            Stage::DuringWrite,
            Stage::AfterWrite,
            Stage::BeforeReplace,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("note.md");
            fs::write(&path, b"original complete note").unwrap();
            fail_at(stage, || {
                assert!(atomic_write(&path, b"replacement", true, false).is_err())
            });
            assert_eq!(fs::read(&path).unwrap(), b"original complete note");
        }
    }
    #[test]
    fn post_commit_sync_failure_reports_uncertainty_and_keeps_whole_new_data() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("note.md");
        fs::write(&path, b"old").unwrap();
        fail_at(Stage::AfterReplace, || {
            let error = atomic_write(&path, b"whole replacement", true, false).unwrap_err();
            assert!(format!("{error:#}").contains("durability is uncertain"));
        });
        assert_eq!(fs::read(&path).unwrap(), b"whole replacement");
    }
    #[test]
    fn no_clobber_and_backups_preserve_prior_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("note.md");
        fs::write(&path, b"old").unwrap();
        assert!(atomic_write(&path, b"new", false, true).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        atomic_write(&path, b"new", true, true).unwrap();
        let directory = temp.path().join(".ph-backups/note.md");
        let files: Vec<_> = fs::read_dir(directory).unwrap().collect();
        assert_eq!(files.len(), 1);
        assert_eq!(fs::read(files[0].as_ref().unwrap().path()).unwrap(), b"old");
    }
    #[test]
    fn failed_backup_aborts_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("note.md");
        fs::write(&path, b"old").unwrap();
        fs::write(temp.path().join(".ph-backups"), b"blocking file").unwrap();
        assert!(atomic_write(&path, b"new", true, true).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
    }
    #[cfg(unix)]
    #[test]
    fn explicit_symlink_target_is_not_followed_or_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside");
        fs::write(&outside, b"secret").unwrap();
        let path = temp.path().join("link");
        std::os::unix::fs::symlink(&outside, &path).unwrap();
        assert!(atomic_write(&path, b"new", true, true).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"secret");
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn canonical_windows_prefix_opens_without_inspecting_partial_drive_path() {
        let temporary = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        let directory = Directory::open(&root, false).unwrap();
        directory
            .write_atomic(std::ffi::OsStr::new("note.md"), b"complete", false, None)
            .unwrap();
        assert_eq!(
            directory.read(std::ffi::OsStr::new("note.md")).unwrap().0,
            b"complete"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_directory_metadata_is_readable_and_junctions_are_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(temporary.path()).unwrap();
        fs::create_dir(root.join("folder")).unwrap();
        fs::create_dir(root.join("outside")).unwrap();
        let directory = Directory::open(&root, false).unwrap();
        assert!(directory
            .metadata(std::ffi::OsStr::new("folder"))
            .unwrap()
            .is_dir());
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(root.join("junction"))
            .arg(root.join("outside"))
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "could not create test junction: {:?}",
            status
        );
        assert!(directory
            .metadata(std::ffi::OsStr::new("junction"))
            .is_err());
        assert!(directory
            .child(std::ffi::OsStr::new("junction"), false)
            .is_err());
    }

    #[test]
    fn directory_listings_have_independent_cursors() {
        let temporary = tempfile::tempdir().unwrap();
        // Directory takes a resolved root. macOS's /var temp alias is resolved
        // before the test introduces its adversarial child symlink.
        let root = fs::canonicalize(temporary.path()).unwrap();
        fs::write(root.join("first"), b"a").unwrap();
        fs::write(root.join("second"), b"b").unwrap();
        let directory = Directory::open(&root, false).unwrap();
        let first = directory.entries().unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(directory.entries().unwrap(), first);
        assert_eq!(
            directory
                .descend(Path::new(""), false)
                .unwrap()
                .entries()
                .unwrap(),
            first
        );
    }

    #[test]
    fn a_second_writer_cannot_acquire_an_existing_store_lock() {
        let temporary = tempfile::tempdir().unwrap();
        // Directory takes a resolved root. macOS's /var temp alias is resolved
        // before the test introduces its adversarial child symlink.
        let root = fs::canonicalize(temporary.path()).unwrap();
        let directory = Directory::open(&root, false).unwrap();
        let lock = directory.lock_file().unwrap();
        let contender = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(".write.lock"))
            .unwrap();
        assert!(contender.try_lock().is_err());
        drop(lock);
        contender.try_lock().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn checked_directory_handle_is_not_redirected_by_parent_symlink_swap() {
        let temporary = tempfile::tempdir().unwrap();
        // Directory takes a resolved root. macOS's /var temp alias is resolved
        // before the test introduces its adversarial child symlink.
        let root = fs::canonicalize(temporary.path()).unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(root.join("prompts")).unwrap();
        fs::write(root.join("prompts/note.md"), b"original").unwrap();
        fs::write(outside.path().join("note.md"), b"outside").unwrap();
        let checked = Directory::open(&root.join("prompts"), false).unwrap();
        fs::rename(root.join("prompts"), root.join("retained")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("prompts")).unwrap();
        checked
            .write_atomic(
                std::ffi::OsStr::new("note.md"),
                b"safe replacement",
                true,
                Some(b"original"),
            )
            .unwrap();
        assert_eq!(
            fs::read(root.join("retained/note.md")).unwrap(),
            b"safe replacement"
        );
        assert_eq!(
            fs::read(outside.path().join("note.md")).unwrap(),
            b"outside"
        );
        assert!(Directory::open(&root.join("prompts"), false).is_err());
    }
}
