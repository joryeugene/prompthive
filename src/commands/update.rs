//! Update only when a Cargo-owned executable can be independently reproduced.
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{BufRead, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";
const HELP: &str = "cannot verify this executable as a crates.io Cargo installation; update it with the original installer";

#[derive(Deserialize)]
struct Metadata {
    installs: BTreeMap<String, Install>,
}
#[derive(Deserialize)]
struct Install {
    bins: Vec<String>,
    features: Vec<String>,
    all_features: bool,
    no_default_features: bool,
    profile: String,
    target: String,
}

fn safe_arg(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}

fn plan(exe: &Path, metadata: &str) -> Result<(PathBuf, Vec<OsString>)> {
    if exe.file_name().and_then(|s| s.to_str()) != Some("ph") {
        bail!(HELP);
    }
    let parent = exe.parent().context(HELP)?;
    if parent.file_name().and_then(|s| s.to_str()) != Some("bin") {
        bail!(HELP);
    }
    let root = parent.parent().context(HELP)?;
    let data: Metadata = serde_json::from_str(metadata).context(HELP)?;
    let expected = format!("prompthive {} ({REGISTRY})", env!("CARGO_PKG_VERSION"));
    let mut owners = data
        .installs
        .iter()
        .filter(|(_, item)| item.bins.iter().any(|bin| bin == "ph"));
    let (key, item) = owners.next().context(HELP)?;
    if key != &expected
        || owners.next().is_some()
        || item.profile != "release"
        || !safe_arg(&item.target)
        || item.features.iter().any(|feature| !safe_arg(feature))
    {
        bail!(HELP);
    }
    let mut args = vec![
        "install".into(),
        "prompthive".into(),
        "--locked".into(),
        "--registry".into(),
        "crates-io".into(),
        "--root".into(),
        root.as_os_str().to_owned(),
        "--target".into(),
        item.target.clone().into(),
    ];
    if item.all_features {
        args.push("--all-features".into());
    }
    if item.no_default_features {
        args.push("--no-default-features".into());
    }
    if !item.features.is_empty() {
        args.extend(["--features".into(), item.features.join(",").into()]);
    }
    Ok((root.to_owned(), args))
}

fn tracking_file(root: &Path, name: &str) -> Result<Vec<u8>> {
    let path = root.join(name);
    if !fs::symlink_metadata(&path)
        .context(HELP)?
        .file_type()
        .is_file()
    {
        bail!(HELP);
    }
    let file = fs::File::open(path).context(HELP)?;
    let mut bytes = Vec::new();
    file.take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .context(HELP)?;
    if bytes.len() > 4 * 1024 * 1024 {
        bail!(HELP);
    }
    Ok(bytes)
}

fn metadata(root: &Path) -> Result<String> {
    String::from_utf8(tracking_file(root, ".crates2.json")?).context(HELP)
}

fn digest(path: &Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(hash.finalize().into())
}

fn installed_version(root: &Path) -> Result<String> {
    let data: Metadata = serde_json::from_str(&metadata(root)?).context(HELP)?;
    let mut owners = data
        .installs
        .iter()
        .filter(|(_, item)| item.bins.iter().any(|bin| bin == "ph"));
    let (key, _) = owners.next().context(HELP)?;
    if owners.next().is_some() {
        bail!(HELP);
    }
    let version = key
        .strip_prefix("prompthive ")
        .and_then(|v| v.strip_suffix(&format!(" ({REGISTRY})")))
        .context(HELP)?;
    if !safe_arg(version) {
        bail!(HELP);
    }
    Ok(version.to_owned())
}

fn cargo(args: &[OsString]) -> Result<()> {
    let status = Command::new("cargo")
        .args(args)
        .stdout(Stdio::null())
        .status()
        .context("could not start Cargo")?;
    if !status.success() {
        bail!("Cargo failed ({status}); inspect its output above");
    }
    Ok(())
}

fn supported_executable(exe: &Path, cargo_home: &Path, home: &Path) -> Result<PathBuf> {
    if !fs::symlink_metadata(exe)
        .context(HELP)?
        .file_type()
        .is_file()
    {
        bail!(HELP);
    }
    // macOS commonly exposes the same temporary directory as /var and
    // /private/var. Compare canonical identities for every path, rather than
    // mixing a canonical Cargo home with an uncanonicalized executable path.
    let exe = fs::canonicalize(exe).context(HELP)?;
    let root = exe.parent().and_then(Path::parent).context(HELP)?;
    let cargo_home = fs::canonicalize(cargo_home).context(HELP)?;
    let home = fs::canonicalize(home).context(HELP)?;
    if cargo_home != root || !root.starts_with(&home) || root == home {
        bail!(HELP);
    }
    Ok(exe)
}

pub fn run() -> Result<()> {
    if cfg!(windows) {
        bail!("update from another terminal with cargo install prompthive --locked, using the original install root and features");
    }
    // Only an explicitly configured Cargo home or the ordinary ~/.cargo is
    // supported. An arbitrary shared --root could belong to another manager.
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".cargo")))
        .context(HELP)?;
    let home = dirs::home_dir().context(HELP)?;
    let exe = supported_executable(&std::env::current_exe().context(HELP)?, &cargo_home, &home)?;
    let root = exe.parent().and_then(Path::parent).context(HELP)?;
    let original_metadata = metadata(root)?;
    let original_tracking = tracking_file(root, ".crates.toml")?;
    let (_, args) = plan(&exe, &original_metadata)?;
    let before = digest(&exe).context(HELP)?;

    // Cargo's metadata alone is not proof of the current bytes. Rebuild the
    // installed version separately and proceed only for an exact byte match.
    // Different toolchains/build paths may not reproduce; those users get a
    // manual-update fallback rather than a forced replacement.
    eprintln!("checking this Cargo installation with a temporary rebuild...");
    let staging = tempfile::Builder::new()
        .prefix("ph-update-check-")
        .tempdir()?;
    let mut check_args = args.clone();
    let root_index = check_args
        .iter()
        .position(|arg| arg == "--root")
        .context(HELP)?
        + 1;
    check_args[root_index] = staging.path().as_os_str().to_owned();
    check_args.extend([
        "--version".into(),
        format!("={}", env!("CARGO_PKG_VERSION")).into(),
    ]);
    cargo(&check_args)?;
    let staged = staging.path().join("bin").join("ph");
    if installed_version(staging.path())? != env!("CARGO_PKG_VERSION") || digest(&staged)? != before
    {
        bail!("the installed binary does not match a reproducible Cargo build; update with the original installer");
    }
    // Build the prospective version in the same temporary root first. Cargo,
    // its configured registry and build environment are trusted inputs.
    let mut newest_args = args.clone();
    newest_args[root_index] = staging.path().as_os_str().to_owned();
    cargo(&newest_args)?;
    let target_version = installed_version(staging.path())?;
    let target_digest = digest(&staged)?;
    let staged_version = Command::new(&staged)
        .arg("--version")
        .output()
        .context("could not verify the staged executable")?;
    if !staged_version.status.success()
        || staged_version.stdout != format!("ph {target_version}\n").as_bytes()
    {
        bail!("the staged executable and Cargo metadata disagree; no update was started");
    }
    if digest(&exe)? != before || metadata(root)? != original_metadata {
        bail!("the installation changed during verification; no update was started");
    }
    if target_version == env!("CARGO_PKG_VERSION") && target_digest == before {
        eprintln!("Cargo reports ph {target_version} is already current");
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        bail!("an interactive terminal is required to confirm ownership before replacing this executable");
    }
    eprint!("Cargo metadata cannot prove who manages {}. Close other installers first. Update your Cargo-managed installation to ph {target_version}? [y/N] ", exe.display());
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().take(16).read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES") {
        bail!("update cancelled; no installation was changed");
    }
    if digest(&exe)? != before || metadata(root)? != original_metadata {
        bail!("the installation changed during verification; no update was started");
    }
    let backup = tempfile::Builder::new()
        .prefix("ph-update-backup-")
        .tempdir_in(root)
        .context("could not create an update recovery directory; no update was started")?;
    fs::copy(&exe, backup.path().join("ph"))?;
    fs::write(
        backup.path().join(".crates2.json"),
        original_metadata.as_bytes(),
    )?;
    fs::write(backup.path().join(".crates.toml"), &original_tracking)?;
    if digest(&backup.path().join("ph"))? != before
        || digest(&exe)? != before
        || metadata(root)? != original_metadata
        || tracking_file(root, ".crates.toml")? != original_tracking
    {
        bail!("the installation changed while preparing recovery; no update was started");
    }
    eprintln!("recovery copy: {}", backup.path().display());
    eprintln!("updating {} with Cargo...", exe.display());
    let mut install_args = args;
    install_args.extend(["--version".into(), format!("={target_version}").into()]);
    let result = (|| -> Result<String> {
        cargo(&install_args)?;
        let version = installed_version(root)
            .context("Cargo finished, but its installed version could not be verified")?;
        if version != target_version || digest(&exe)? != target_digest {
            bail!(
                "Cargo finished, but the executable could not be verified against the staged build"
            );
        }
        Ok(version)
    })();
    let version = match result {
        Ok(version) => version,
        Err(error) => {
            let path = backup.keep();
            bail!("{error:#}. The installation may have changed. A copy of the previous executable and Cargo metadata is saved at {}; inspect it before recovery; nothing was automatically restored", path.display());
        }
    };
    if version == env!("CARGO_PKG_VERSION") {
        eprintln!("Cargo reports ph {version} is already current");
    } else {
        eprintln!("updated: ph {version}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metadata(source: &str, features: &str) -> String {
        format!(
            r#"{{"installs":{{"prompthive {} ({source})":{{"bins":["ph"],"features":{features},"all_features":false,"no_default_features":true,"profile":"release","target":"x86_64-unknown-linux-gnu"}}}}}}"#,
            env!("CARGO_PKG_VERSION")
        )
    }
    #[test]
    fn preserves_install_configuration() {
        let (root, args) = plan(
            Path::new("/tmp/example/bin/ph"),
            &metadata(REGISTRY, "[\"tui\"]"),
        )
        .unwrap();
        assert_eq!(root, Path::new("/tmp/example"));
        assert!(args.contains(&"--no-default-features".into()));
        assert!(args
            .windows(2)
            .any(|p| p[0] == "--features" && p[1] == "tui"));
        assert!(!args.contains(&"--force".into()));
    }
    #[test]
    fn rejects_unknown_source_and_options() {
        assert!(plan(
            Path::new("/usr/bin/ph"),
            &metadata("path+file:///tmp/source", "[]")
        )
        .is_err());
        assert!(plan(
            Path::new("/tmp/example/bin/ph"),
            &metadata(REGISTRY, "[\"--force\"]")
        )
        .is_err());
        assert!(plan(Path::new("/tmp/ph"), &metadata(REGISTRY, "[]")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn executable_and_cargo_home_accept_equivalent_parent_aliases() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        let home = real.join("home");
        let cargo = home.join("cargo");
        fs::create_dir_all(cargo.join("bin")).unwrap();
        fs::write(cargo.join("bin/ph"), b"fixture").unwrap();
        let alias = temp.path().join("alias");
        symlink(&real, &alias).unwrap();
        let alias_home = alias.join("home");
        let alias_exe = alias_home.join("cargo/bin/ph");
        assert_ne!(
            alias_exe.parent().unwrap().parent().unwrap(),
            fs::canonicalize(&cargo).unwrap()
        );
        let resolved = supported_executable(&alias_exe, &cargo, &alias_home).unwrap();
        assert_eq!(resolved, fs::canonicalize(cargo.join("bin/ph")).unwrap());
        let other = temp.path().join("other");
        fs::create_dir(&other).unwrap();
        assert!(supported_executable(&alias_exe, &cargo, &other).is_err());
        assert!(supported_executable(&alias_exe, &other, &alias_home).is_err());
        let linked_exe = cargo.join("bin/linked-ph");
        symlink(cargo.join("bin/ph"), &linked_exe).unwrap();
        assert!(supported_executable(&linked_exe, &cargo, &alias_home).is_err());
    }
}
