use crate::{matching, portability, Storage};
use anyhow::Result;
#[derive(Debug)]
pub struct CliFailure {
    pub code: i32,
    pub message: String,
}
impl std::fmt::Display for CliFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for CliFailure {}
pub fn fail(code: i32, message: impl Into<String>) -> anyhow::Error {
    CliFailure {
        code,
        message: message.into(),
    }
    .into()
}
pub fn usage(error: impl std::fmt::Display) -> anyhow::Error {
    fail(2, error.to_string())
}
pub fn resolve_prompt_name(storage: &Storage, query: &str) -> Result<String> {
    Storage::validate_name(query).map_err(usage)?;
    // An existing canonical file owns its identity even if optional legacy
    // metadata is malformed, unreadable, or contains a conflicting binding.
    match storage.read_raw(query) {
        Ok(_) => {
            match portability::read_verified_compatibility(storage.base_dir()) {
                Ok(bindings) if bindings.get(query).is_some_and(|target| target != query) =>
                    eprintln!("Compatibility binding {query:?} is disabled because a canonical prompt has that name."),
                Err(_) => eprintln!("Compatibility metadata is unavailable; using the exact canonical prompt."),
                _ => {},
            }
            return Ok(query.into());
        }
        Err(error)
            if error.chain().any(|e| {
                e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            }) => {}
        Err(error) => return Err(error),
    }
    let bindings = portability::read_verified_compatibility(storage.base_dir())?;
    if let Some(target) = bindings.get(query) {
        storage.read_raw(target).map_err(|error| {
            error.context(format!("cannot read compatibility target {target:?}"))
        })?;
        return Ok(target.clone());
    }
    let suggestions = matching::ranked_suggestions(query, &storage.list_prompts()?);
    let list = suggestions
        .iter()
        .take(8)
        .map(|n| format!("  {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(fail(
        2,
        format!(
            "No exact prompt named {query:?}. Use ph u FULL_NAME, or ph f {query:?} to discover prompts.{}",
            if list.is_empty() {
                String::new()
            } else {
                format!("\nCandidate names:\n{list}")
            }
        ),
    ))
}
pub fn parse_variables(values: &[String]) -> Result<std::collections::BTreeMap<String, String>> {
    let mut result = std::collections::BTreeMap::new();
    for value in values {
        let (name, text) = value
            .split_once('=')
            .ok_or_else(|| fail(2, "variables use -v NAME=value"))?;
        if name.is_empty() || result.insert(name.into(), text.into()).is_some() {
            return Err(fail(
                2,
                format!("duplicate or empty variable name {name:?}"),
            ));
        }
    }
    Ok(result)
}
pub fn write_stdout(text: &str) -> Result<()> {
    use std::io::Write;
    crate::limits::check_text(text, "stdout payload")?;
    let mut output = std::io::stdout().lock();
    output.write_all(text.as_bytes())?;
    output.flush()?;
    Ok(())
}
pub fn read_input(argument: Option<&str>) -> Result<String> {
    use std::io::IsTerminal;
    if let Some(argument) = argument {
        crate::limits::check_text(argument, "input argument")?;
    }
    let mut bytes = Vec::new();
    if !std::io::stdin().is_terminal() {
        bytes = crate::limits::read(std::io::stdin().lock(), crate::limits::TEXT_BYTES, "stdin")?;
    }
    if argument.is_some() && !bytes.is_empty() {
        return Err(fail(
            2,
            "input was supplied both as an argument and through stdin; choose one",
        ));
    }
    if let Some(argument) = argument {
        return Ok(argument.into());
    }
    String::from_utf8(bytes).map_err(|_| fail(2, "stdin must contain UTF-8 text"))
}
pub fn error_code(error: &anyhow::Error) -> i32 {
    if error
        .chain()
        .any(|cause| cause.downcast_ref::<crate::limits::LimitError>().is_some())
    {
        2
    } else if error
        .downcast_ref::<crate::persistence::ConflictError>()
        .is_some()
    {
        3
    } else if let Some(failure) = error.downcast_ref::<CliFailure>() {
        failure.code
    } else if error.chain().any(|e| {
        e.downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe)
    }) {
        0
    } else {
        1
    }
}
