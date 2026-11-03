//! Resolve all local sources and validate templates before producing any output.

use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};

use crate::storage::{PromptMetadata, Storage};
use crate::template::{validate_declarations, PreparedTemplate, RenderContext, Variables};

pub struct Composer {
    storage: Storage,
}

struct Source {
    name: String,
    metadata: PromptMetadata,
    body: String,
    bytes: usize,
}

/// Paths are explicit; a dotted prompt ID is still a prompt ID.
pub fn is_explicit_file(name: &str) -> bool {
    std::path::Path::new(name).is_absolute()
        || (cfg!(windows) && (name.starts_with(".\\") || name.starts_with("..\\")))
        || name.starts_with("./")
        || name.starts_with("../")
        || name.starts_with('/')
        || name.starts_with('~')
}

impl Composer {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub fn render(&self, names: &[String], input: &str, supplied: &Variables) -> Result<String> {
        if names.is_empty() {
            bail!("no prompts specified for composition");
        }
        crate::limits::check_text(input, "composition input")?;
        let sources = self.load_sources(names)?;
        let prepared = self.prepare(&sources, supplied)?;
        let mut result = input.to_string();
        for template in prepared {
            result = template.render(&result)?;
        }
        Ok(result)
    }

    /// Directives are prefixed fragments of one template, not extra chain stages.
    pub fn render_with_directives(
        &self,
        name: &str,
        directives: &[String],
        input: &str,
        supplied: &Variables,
    ) -> Result<String> {
        crate::limits::check_text(input, "composition input")?;
        let mut names = directives.to_vec();
        names.push(name.to_owned());
        let sources = self.load_sources(&names)?;
        let mut prepared = self.prepare(&sources, supplied)?;
        let mut combined = prepared.pop().expect("target source always present");
        for directive in prepared.into_iter().rev() {
            combined.prepend(directive)?;
        }
        combined.render(input)
    }

    /// Kept for programmatic callers; stdin and destinations belong to the CLI.
    pub fn compose_pipe(&self, names: &[String], input: &str) -> Result<String> {
        self.render(names, input, &BTreeMap::new())
    }

    pub fn compose(&self, names: &[String]) -> Result<String> {
        self.compose_pipe(names, "")
    }

    /// Collect only absent required values; no context is captured or text rendered here.
    pub fn required_variables(
        &self,
        names: &[String],
        supplied: &Variables,
    ) -> Result<Vec<String>> {
        if names.is_empty() {
            bail!("no prompts specified for composition");
        }
        let sources = self.load_sources(names)?;
        self.validate_supplied(&sources, supplied)?;
        Ok(sources
            .iter()
            .flat_map(|source| source.metadata.vars.iter())
            .filter(|(name, default)| default.is_none() && !supplied.contains_key(*name))
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    fn load_sources(&self, names: &[String]) -> Result<Vec<Source>> {
        let mut budget =
            crate::limits::Budget::new(crate::limits::IMPORT_BYTES, "composition source batch");
        let mut sources = Vec::new();
        for name in names {
            let source = self.load_source(name)?;
            budget.add(source.bytes)?;
            sources.push(source);
        }
        Ok(sources)
    }

    fn load_source(&self, name: &str) -> Result<Source> {
        let bytes = if is_explicit_file(name) {
            let expanded = shellexpand::tilde(name);
            crate::limits::read_file(expanded.as_ref(), "local template source")?
        } else {
            self.storage
                .read_raw(name)
                .with_context(|| format!("cannot read prompt {name:?}"))?
        };
        let content = std::str::from_utf8(&bytes).context("template source is not UTF-8")?;
        let (metadata, body) = self
            .storage
            .parse_prompt_content(content)
            .with_context(|| format!("invalid template source {name:?}"))?;
        Ok(Source {
            name: name.to_string(),
            metadata,
            body,
            bytes: bytes.len(),
        })
    }

    fn validate_supplied(&self, sources: &[Source], supplied: &Variables) -> Result<()> {
        let mut declared = BTreeSet::new();
        for source in sources {
            validate_declarations(&source.metadata.vars)
                .with_context(|| format!("invalid variables in {:?}", source.name))?;
            declared.extend(source.metadata.vars.keys().cloned());
        }
        for name in supplied.keys() {
            if !declared.contains(name) {
                bail!("variable {name:?} is not declared by any selected template");
            }
        }
        Ok(())
    }

    fn prepare(&self, sources: &[Source], supplied: &Variables) -> Result<Vec<PreparedTemplate>> {
        self.validate_supplied(sources, supplied)?;
        let mut context = RenderContext::capture();
        let mut budget =
            crate::limits::Budget::new(crate::limits::IMPORT_BYTES, "compiled composition");
        let mut prepared = Vec::new();
        for source in sources {
            let template = PreparedTemplate::compile(
                &source.body,
                &source.metadata.vars,
                supplied,
                &mut context,
            )
            .with_context(|| format!("cannot render {:?}", source.name))?;
            budget.add(template.compiled_bytes()?)?;
            prepared.push(template);
        }
        Ok(prepared)
    }
}

pub fn parse_prompt_list(input: &str) -> Vec<String> {
    if input.trim().is_empty() {
        return Vec::new();
    }
    input
        .split(',')
        .map(str::trim)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::Declarations;
    use std::fs;
    use tempfile::TempDir;

    fn store() -> (TempDir, Storage) {
        let directory = TempDir::new().unwrap();
        let storage = Storage::new_with_base(directory.path().join("store")).unwrap();
        storage.init().unwrap();
        (directory, storage)
    }

    fn save(storage: &Storage, name: &str, body: &str, vars: Declarations) {
        let metadata = PromptMetadata {
            id: name.into(),
            description: name.into(),
            vars,
            ..Default::default()
        };
        storage.write_prompt(name, &metadata, body).unwrap();
    }

    #[test]
    fn missing_values_are_sorted_stage_local_and_do_not_capture_context() {
        let (_directory, storage) = store();
        save(
            &storage,
            "a",
            "{tone} {env:PROMPTHIVE_REQUIRED_PROBE_UNSET}",
            BTreeMap::from([("tone".into(), None), ("lang".into(), Some("Rust".into()))]),
        );
        save(
            &storage,
            "b",
            "{level} {tone}",
            BTreeMap::from([
                ("tone".into(), Some("brief".into())),
                ("level".into(), None),
            ]),
        );
        let composer = Composer::new(storage);
        let names = vec!["a".into(), "b".into()];
        assert_eq!(
            composer
                .required_variables(&names, &Variables::new())
                .unwrap(),
            vec!["level", "tone"]
        );
        assert_eq!(
            composer
                .required_variables(&names, &BTreeMap::from([("tone".into(), "".into())]))
                .unwrap(),
            vec!["level"]
        );
        assert!(composer
            .required_variables(
                &names,
                &BTreeMap::from([("unknown".into(), "value".into())])
            )
            .is_err());
    }

    #[test]
    fn chain_order_and_previous_output_are_literal() {
        let (_directory, storage) = store();
        save(&storage, "a", "A {input}", Declarations::new());
        save(&storage, "b", "B {content}", Declarations::new());
        let composer = Composer::new(storage);
        let data = "{date} {env:UNAVAILABLE_COMPOSITION_INPUT}";
        assert_eq!(
            composer
                .render(&["a".into(), "b".into()], data, &Variables::new())
                .unwrap(),
            format!("B A {data}")
        );
    }

    #[test]
    fn declarations_are_stage_local_and_values_override_each_stage() {
        let (_directory, storage) = store();
        save(
            &storage,
            "a",
            "{tone} {input}",
            BTreeMap::from([("tone".into(), Some("first".into()))]),
        );
        save(
            &storage,
            "b",
            "{tone} {input}",
            BTreeMap::from([("tone".into(), Some("second".into()))]),
        );
        let composer = Composer::new(storage);
        let names = ["a".into(), "b".into()];
        assert_eq!(
            composer.render(&names, "data", &Variables::new()).unwrap(),
            "second first data"
        );
        assert_eq!(
            composer
                .render(
                    &names,
                    "data",
                    &BTreeMap::from([("tone".into(), "explicit".into())])
                )
                .unwrap(),
            "explicit explicit data"
        );
    }

    #[test]
    fn all_sources_and_required_values_are_checked_before_result() {
        let (_directory, storage) = store();
        save(&storage, "a", "A {input}", Declarations::new());
        save(
            &storage,
            "b",
            "{topic} {input}",
            BTreeMap::from([("topic".into(), None)]),
        );
        let composer = Composer::new(storage);
        assert!(composer
            .render(&["a".into(), "missing".into()], "data", &Variables::new())
            .is_err());
        assert!(composer
            .render(&["a".into(), "b".into()], "data", &Variables::new())
            .is_err());
        assert_eq!(
            composer
                .render(
                    &["a".into(), "b".into()],
                    "data",
                    &BTreeMap::from([("topic".into(), "T".into())])
                )
                .unwrap(),
            "T A data"
        );
        assert!(composer
            .render(
                &["a".into()],
                "data",
                &BTreeMap::from([("typo".into(), "value".into())])
            )
            .is_err());
    }

    #[test]
    fn context_snapshot_is_shared_across_chain_stages() {
        let (_directory, storage) = store();
        save(&storage, "a", "{uuid}|{input}", Declarations::new());
        save(&storage, "b", "{uuid}|{input}", Declarations::new());
        let result = Composer::new(storage)
            .render(&["a".into(), "b".into()], "tail", &Variables::new())
            .unwrap();
        let parts: Vec<_> = result.split('|').collect();
        assert_eq!(parts[0], parts[1]);
        assert_eq!(parts[2], "tail");
    }

    #[test]
    fn directives_preserve_order_and_append_context_once() {
        let (_directory, storage) = store();
        save(&storage, "d1", "First", Declarations::new());
        save(&storage, "d2", "Second", Declarations::new());
        save(&storage, "target", "Target", Declarations::new());
        let composer = Composer::new(storage.clone());
        assert_eq!(
            composer
                .render_with_directives(
                    "target",
                    &["d1".into(), "d2".into()],
                    "  data\n",
                    &Variables::new()
                )
                .unwrap(),
            "First\n\nSecond\n\nTarget\n\n  data\n"
        );
        save(&storage, "target", "Target {input}", Declarations::new());
        assert_eq!(
            composer
                .render_with_directives("target", &["d1".into()], "data", &Variables::new())
                .unwrap(),
            "First\n\nTarget data"
        );
    }

    #[test]
    fn explicit_file_sources_use_metadata_and_dotted_ids_stay_ids() {
        let (directory, storage) = store();
        save(
            &storage,
            "review.v2",
            "Version two: {input}",
            Declarations::new(),
        );
        let file = directory.path().join("source.md");
        fs::write(&file, "---\nid: external\ndescription: external\nvars:\n  topic: null\n---\n\n{topic} {input}").unwrap();
        let composer = Composer::new(storage);
        assert_eq!(
            composer
                .render(&["review.v2".into()], "data", &Variables::new())
                .unwrap(),
            "Version two: data"
        );
        assert_eq!(
            composer
                .render(
                    &[file.to_string_lossy().into_owned()],
                    "data",
                    &BTreeMap::from([("topic".into(), "File".into())])
                )
                .unwrap(),
            "File data"
        );
        for name in ["./a.md", "../a.md", "/a.md", "~/a.md"] {
            assert!(is_explicit_file(name));
        }
        for name in ["review.v2", "bank/a.md", "https://example.test/a.md"] {
            assert!(!is_explicit_file(name));
        }
    }

    #[test]
    fn comma_list_remains_the_existing_simple_grammar() {
        assert_eq!(parse_prompt_list("a, b ,c"), vec!["a", "b", "c"]);
        assert!(parse_prompt_list("").is_empty());
    }

    #[test]
    fn incomplete_composition_does_not_silently_drop_a_stage() {
        let (_directory, storage) = store();
        save(&storage, "a", "A {input}", Declarations::new());
        let composer = Composer::new(storage);
        assert!(composer
            .render(&parse_prompt_list("a,"), "data", &Variables::new())
            .is_err());
        assert!(composer
            .render(&parse_prompt_list("a,,a"), "data", &Variables::new())
            .is_err());
    }
}
