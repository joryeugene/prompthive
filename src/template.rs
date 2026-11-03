//! Single-pass template rendering. Inserted values are text, never new template source.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Local, Utc};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub type Declarations = BTreeMap<String, Option<String>>;
pub type Variables = BTreeMap<String, String>;

const INPUT_SLOTS: &[&str] = &["input", "INPUT", "content", "CONTENT"];
const SYSTEM_SLOTS: &[&str] = &[
    "date",
    "time",
    "datetime",
    "timestamp",
    "iso_date",
    "user",
    "hostname",
    "uuid",
    "cwd",
    "pwd",
    "git_branch",
    "git_status",
    "git_hash",
];

pub fn is_reserved_name(name: &str) -> bool {
    INPUT_SLOTS.contains(&name) || SYSTEM_SLOTS.contains(&name) || name.starts_with("env:")
}

pub fn validate_declarations(declared: &Declarations) -> Result<()> {
    for name in declared.keys() {
        let mut chars = name.chars();
        if !matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            bail!("invalid variable name {name:?}; use an ASCII identifier");
        }
        if is_reserved_name(name) {
            bail!("variable {name:?} is reserved");
        }
    }
    Ok(())
}

/// Programmatic defaults remain explicit; no configuration file is loaded here.
#[derive(Default)]
pub struct TemplateProcessor {
    custom_variables: HashMap<String, String>,
}

impl TemplateProcessor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn process(&self, template: &str, input: &str) -> Result<String> {
        let declared = self
            .custom_variables
            .iter()
            .map(|(name, value)| (name.clone(), Some(value.clone())))
            .collect();
        self.process_with_variables(template, input, &declared, &Variables::new())
    }

    pub fn process_with_variables(
        &self,
        template: &str,
        input: &str,
        declared: &Declarations,
        supplied: &Variables,
    ) -> Result<String> {
        validate_declarations(declared)?;
        for name in supplied.keys() {
            if !declared.contains_key(name) {
                bail!("variable {name:?} is not declared by this template");
            }
        }
        let mut context = RenderContext::capture();
        PreparedTemplate::compile(template, declared, supplied, &mut context)?.render(input)
    }

    pub fn set_custom_variable(&mut self, name: &str, value: &str) {
        self.custom_variables.insert(name.into(), value.into());
    }

    pub fn remove_custom_variable(&mut self, name: &str) {
        self.custom_variables.remove(name);
    }

    pub fn get_custom_variables(&self) -> &HashMap<String, String> {
        &self.custom_variables
    }
}

/// One instance is shared by every source in an invocation.
pub(crate) struct RenderContext {
    now: DateTime<Utc>,
    directory: Option<PathBuf>,
    captured: Variables,
}

impl RenderContext {
    pub(crate) fn capture() -> Self {
        Self {
            now: Utc::now(),
            directory: env::current_dir().ok(),
            captured: Variables::new(),
        }
    }

    fn value(&mut self, token: &str) -> Result<String> {
        if let Some(value) = self.captured.get(token) {
            return Ok(value.clone());
        }
        let local = self.now.with_timezone(&Local);
        let value = match token {
            "date" => self.now.format("%Y-%m-%d").to_string(),
            "time" => local.format("%H:%M:%S").to_string(),
            "datetime" => local.format("%Y-%m-%d %H:%M:%S").to_string(),
            "timestamp" => self.now.timestamp().to_string(),
            "iso_date" => self.now.to_rfc3339(),
            "user" => env::var("USER")
                .or_else(|_| env::var("USERNAME"))
                .context("cannot determine the current user")?,
            "hostname" => gethostname::gethostname().to_string_lossy().into_owned(),
            "uuid" => uuid::Uuid::new_v4().to_string(),
            "cwd" | "pwd" => {
                let directory = self
                    .directory
                    .as_ref()
                    .context("cannot determine the current directory")?;
                let part = if token == "pwd" {
                    directory.file_name().unwrap_or(directory.as_os_str())
                } else {
                    directory.as_os_str()
                };
                part.to_string_lossy().into_owned()
            }
            "git_branch" => self.git(&["rev-parse", "--abbrev-ref", "HEAD"], token)?,
            "git_hash" => self.git(&["rev-parse", "--short", "HEAD"], token)?,
            "git_status" => {
                if self.git(&["status", "--porcelain"], token)?.is_empty() {
                    "clean".into()
                } else {
                    "dirty".into()
                }
            }
            name if name.starts_with("env:") => {
                let name = &name[4..];
                let mut chars = name.chars();
                if !matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
                    || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    bail!("invalid environment variable name {name:?}");
                }
                env::var(name).with_context(|| {
                    format!("required environment variable {name:?} is unavailable")
                })?
            }
            _ => unreachable!("only recognized source tokens request context"),
        };
        self.captured.insert(token.into(), value.clone());
        Ok(value)
    }

    fn git(&self, args: &[&str], token: &str) -> Result<String> {
        let directory = self
            .directory
            .as_ref()
            .context("cannot determine the current directory")?;
        let mut child = Command::new("git")
            .args([
                "--no-optional-locks",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.untrackedCache=false",
            ])
            .args(args)
            .current_dir(directory)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("cannot read requested Git context {{{token}}}"))?;
        let output = crate::limits::read(
            child.stdout.take().expect("piped Git stdout"),
            crate::limits::TEXT_BYTES,
            "Git context",
        );
        let bytes = match output {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        if !child.wait()?.success() {
            bail!("requested Git context {{{token}}} is unavailable in this directory");
        }
        let text = String::from_utf8(bytes).context("Git context is not valid UTF-8")?;
        Ok(text.trim_end_matches(['\r', '\n']).to_string())
    }
}

#[derive(Debug)]
enum Part {
    Text(String),
    Input,
}

/// Compile only source tokens; rendering can never reinterpret inserted text.
pub(crate) struct PreparedTemplate {
    parts: Vec<Part>,
    has_input: bool,
    text_bytes: usize,
}

impl PreparedTemplate {
    pub(crate) fn compiled_bytes(&self) -> Result<usize> {
        self.parts
            .len()
            .checked_mul(std::mem::size_of::<Part>())
            .and_then(|size| size.checked_add(self.text_bytes))
            .ok_or_else(|| {
                crate::limits::exceeded("compiled composition", crate::limits::IMPORT_BYTES)
            })
    }
    pub(crate) fn compile(
        source: &str,
        declared: &Declarations,
        supplied: &Variables,
        context: &mut RenderContext,
    ) -> Result<Self> {
        crate::limits::check_text(source, "template source")?;
        validate_declarations(declared)?;
        let mut values = BTreeMap::new();
        for (name, default) in declared {
            let value = supplied.get(name).or(default.as_ref()).with_context(|| {
                format!("required variable {name:?} is missing; supply -v {name}=VALUE")
            })?;
            crate::limits::check_text(value, "template value")?;
            values.insert(name.as_str(), value.as_str());
        }

        let mut prepared = Self {
            parts: Vec::new(),
            has_input: false,
            text_bytes: 0,
        };
        let mut offset = 0;
        while let Some(relative) = source[offset..].find('{') {
            let start = offset + relative;
            prepared.push_text(&source[offset..start])?;
            if source[start..].starts_with("{{") {
                if let Some(relative_end) = source[start + 2..].find("}}") {
                    let end = start + 2 + relative_end;
                    let token = &source[start + 2..end];
                    if is_reserved_name(token) || declared.contains_key(token) {
                        prepared.push_text(&format!("{{{token}}}"))?;
                    } else {
                        prepared.push_text(&source[start..end + 2])?;
                    }
                    offset = end + 2;
                    continue;
                }
                prepared.push_text(&source[start..])?;
                offset = source.len();
                break;
            }
            let Some(relative_end) = source[start + 1..].find('}') else {
                prepared.push_text(&source[start..])?;
                offset = source.len();
                break;
            };
            let end = start + 1 + relative_end;
            let token = &source[start + 1..end];
            if token.contains('{') {
                prepared.push_text("{")?;
                offset = start + 1;
                continue;
            }
            if INPUT_SLOTS.contains(&token) {
                crate::limits::check_parts(prepared.parts.len() + 1)?;
                prepared.parts.push(Part::Input);
                prepared.has_input = true;
            } else if SYSTEM_SLOTS.contains(&token) || token.starts_with("env:") {
                prepared.push_text(&context.value(token)?)?;
            } else if let Some(value) = values.get(token) {
                prepared.push_text(value)?;
            } else {
                prepared.push_text(&source[start..=end])?;
            }
            offset = end + 1;
        }
        prepared.push_text(&source[offset..])?;
        Ok(prepared)
    }

    fn push_text(&mut self, value: &str) -> Result<()> {
        if value.is_empty() {
            return Ok(());
        }
        let size = self.text_bytes.checked_add(value.len()).ok_or_else(|| {
            crate::limits::exceeded("compiled template text", crate::limits::TEXT_BYTES)
        })?;
        crate::limits::check_len(size, crate::limits::TEXT_BYTES, "compiled template text")?;
        match self.parts.last_mut() {
            Some(Part::Text(previous)) => {
                crate::limits::push_str(previous, value, "compiled template text")?
            }
            _ => {
                crate::limits::check_parts(self.parts.len() + 1)?;
                self.parts.push(Part::Text(value.to_string()));
            }
        }
        self.text_bytes = size;
        Ok(())
    }

    pub(crate) fn prepend(&mut self, mut before: Self) -> Result<()> {
        before.push_text("\n\n")?;
        let size = before
            .text_bytes
            .checked_add(self.text_bytes)
            .ok_or_else(|| {
                crate::limits::exceeded("combined template", crate::limits::TEXT_BYTES)
            })?;
        crate::limits::check_len(size, crate::limits::TEXT_BYTES, "combined template")?;
        crate::limits::check_parts(before.parts.len() + self.parts.len())?;
        before.parts.append(&mut self.parts);
        self.parts = before.parts;
        self.has_input |= before.has_input;
        self.text_bytes = size;
        Ok(())
    }

    pub(crate) fn render(&self, input: &str) -> Result<String> {
        crate::limits::check_text(input, "template input")?;
        let mut size = self.text_bytes;
        for part in &self.parts {
            if matches!(part, Part::Input) {
                size = size.checked_add(input.len()).ok_or_else(|| {
                    crate::limits::exceeded("rendered output", crate::limits::TEXT_BYTES)
                })?;
            }
        }
        if !self.has_input && !input.is_empty() {
            size = size
                .checked_add(2)
                .and_then(|size| size.checked_add(input.len()))
                .ok_or_else(|| {
                    crate::limits::exceeded("rendered output", crate::limits::TEXT_BYTES)
                })?;
        }
        crate::limits::check_len(size, crate::limits::TEXT_BYTES, "rendered output")?;
        let mut result = String::new();
        result
            .try_reserve_exact(size)
            .context("cannot allocate bounded rendered output")?;
        for part in &self.parts {
            match part {
                Part::Text(text) => result.push_str(text),
                Part::Input => result.push_str(input),
            }
        }
        if !self.has_input && !input.is_empty() {
            result.push_str("\n\n");
            result.push_str(input);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_input_spellings_preserve_empty_and_whitespace() {
        let processor = TemplateProcessor::new();
        for name in INPUT_SLOTS {
            let source = format!("prefix {{{name}}} suffix");
            assert_eq!(
                processor.process(&source, "  α\n\n").unwrap(),
                "prefix   α\n\n suffix"
            );
            assert_eq!(processor.process(&source, "").unwrap(), "prefix  suffix");
        }
    }

    #[test]
    fn input_is_not_reparsed_as_environment_or_system_source() {
        let source = "Review {input}";
        let data = "{env:PROMPTHIVE_LITERAL_INPUT_MUST_NOT_READ} {date} $(false) {input}";
        assert_eq!(
            TemplateProcessor::new().process(source, data).unwrap(),
            format!("Review {data}")
        );
    }

    #[test]
    fn values_and_defaults_are_literal() {
        let processor = TemplateProcessor::new();
        let declared = BTreeMap::from([("tone".into(), Some("{date}".into()))]);
        assert_eq!(
            processor
                .process_with_variables("{tone}: {input}", "text", &declared, &Variables::new())
                .unwrap(),
            "{date}: text"
        );
        let values = BTreeMap::from([("tone".into(), "{input} {env:SHOULD_NOT_READ}".into())]);
        assert_eq!(
            processor
                .process_with_variables("{tone}: {input}", "text", &declared, &values)
                .unwrap(),
            "{input} {env:SHOULD_NOT_READ}: text"
        );
    }

    #[test]
    fn undeclared_code_and_json_braces_stay_literal() {
        let source = "fn f() {foo}\n{\"name\": \"value\"}\n{{unknown}}";
        assert_eq!(
            TemplateProcessor::new().process(source, "").unwrap(),
            source
        );
        assert_eq!(
            TemplateProcessor::new()
                .process("{\"value\": \"{input}\"}", "data")
                .unwrap(),
            "{\"value\": \"data\"}"
        );
    }

    #[test]
    fn escaped_tokens_do_not_request_context_or_consume_input() {
        let processor = TemplateProcessor::new();
        assert_eq!(
            processor
                .process(
                    "{{input}} {{date}} {{env:UNAVAILABLE_ESCAPED_NAME}}",
                    "data"
                )
                .unwrap(),
            "{input} {date} {env:UNAVAILABLE_ESCAPED_NAME}\n\ndata"
        );
        let declarations = BTreeMap::from([("topic".into(), Some("ignored".into()))]);
        assert_eq!(
            processor
                .process_with_variables("{{topic}}", "", &declarations, &Variables::new())
                .unwrap(),
            "{topic}"
        );
        assert_eq!(
            processor
                .process("{{env:UNAVAILABLE_MALFORMED_ESCAPE}", "")
                .unwrap(),
            "{{env:UNAVAILABLE_MALFORMED_ESCAPE}"
        );
    }

    #[test]
    fn required_unknown_invalid_and_reserved_variables_fail() {
        let processor = TemplateProcessor::new();
        let declared = BTreeMap::from([("topic".into(), None)]);
        assert!(processor
            .process_with_variables("{topic}", "", &declared, &Variables::new())
            .unwrap_err()
            .to_string()
            .contains("topic"));
        let supplied = BTreeMap::from([("typo".into(), "value".into())]);
        assert!(processor
            .process_with_variables("text", "", &Declarations::new(), &supplied)
            .is_err());
        for name in ["input", "date", "has-dash", "1name", "", "env:NAME"] {
            assert!(
                validate_declarations(&BTreeMap::from([(name.into(), Some("x".into()))])).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn context_without_input_slot_does_not_drop_input() {
        let result = TemplateProcessor::new()
            .process("Today {date}; JSON {foo}", "  context\n")
            .unwrap();
        assert!(result.ends_with("; JSON {foo}\n\n  context\n"));
    }

    #[test]
    fn repeated_context_values_are_captured_once() {
        let result = TemplateProcessor::new()
            .process("{uuid}|{uuid}|{timestamp}|{timestamp}", "")
            .unwrap();
        let parts: Vec<_> = result.split('|').collect();
        assert_eq!(parts[0], parts[1]);
        assert_eq!(parts[2], parts[3]);
    }

    #[test]
    fn environment_value_is_cached_and_opaque() {
        let name = format!("PROMPTHIVE_TEMPLATE_TEST_{}", uuid::Uuid::new_v4().simple());
        env::set_var(&name, "{date}");
        let mut context = RenderContext::capture();
        let token = format!("env:{name}");
        assert_eq!(context.value(&token).unwrap(), "{date}");
        env::set_var(&name, "changed");
        assert_eq!(context.value(&token).unwrap(), "{date}");
        let template = format!("{{{token}}}");
        let prepared = PreparedTemplate::compile(
            &template,
            &Declarations::new(),
            &Variables::new(),
            &mut context,
        )
        .unwrap();
        assert_eq!(prepared.render("").unwrap(), "{date}");
        env::remove_var(&name);
        assert!(RenderContext::capture().value(&token).is_err());
    }

    #[test]
    fn requested_missing_git_context_fails() {
        let directory = tempfile::tempdir().unwrap();
        let mut context = RenderContext::capture();
        context.directory = Some(directory.path().to_path_buf());
        assert!(context.value("git_branch").is_err());
    }
}
