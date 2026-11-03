//! Resolve all local sources and validate templates before producing any output.

use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use crate::storage::{PromptMetadata, Storage};
use crate::template::{validate_declarations, PreparedTemplate, RenderContext, Variables};

pub struct Composer {
    storage: Storage,
}

struct Source {
    name: String,
    metadata: PromptMetadata,
    body: String,
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
        let sources = names
            .iter()
            .map(|name| self.load_source(name))
            .collect::<Result<Vec<_>>>()?;
        let prepared = self.prepare(&sources, supplied)?;
        let mut result = input.to_string();
        for template in prepared {
            result = template.render(&result);
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
        let mut sources = directives
            .iter()
            .map(|name| self.load_source(name))
            .collect::<Result<Vec<_>>>()?;
        sources.push(self.load_source(name)?);
        let mut prepared = self.prepare(&sources, supplied)?;
        let mut combined = prepared.pop().expect("target source always present");
        for directive in prepared.into_iter().rev() {
            combined.prepend(directive);
        }
        Ok(combined.render(input))
    }

    /// Kept for programmatic callers; stdin and destinations belong to the CLI.
    pub fn compose_pipe(&self, names: &[String], input: &str) -> Result<String> {
        self.render(names, input, &BTreeMap::new())
    }

    pub fn compose(&self, names: &[String]) -> Result<String> {
        self.compose_pipe(names, "")
    }

    fn load_source(&self, name: &str) -> Result<Source> {
        let (metadata, body) = if is_explicit_file(name) {
            let expanded = shellexpand::tilde(name);
            let content = fs::read_to_string(expanded.as_ref())
                .with_context(|| format!("cannot read local template source {name:?}"))?;
            self.storage
                .parse_prompt_content(&content)
                .with_context(|| format!("invalid local template source {name:?}"))?
        } else {
            self.storage
                .read_prompt(name)
                .with_context(|| format!("cannot read prompt {name:?}"))?
        };
        Ok(Source {
            name: name.to_string(),
            metadata,
            body,
        })
    }

    fn prepare(&self, sources: &[Source], supplied: &Variables) -> Result<Vec<PreparedTemplate>> {
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
        let mut context = RenderContext::capture();
        sources
            .iter()
            .map(|source| {
                PreparedTemplate::compile(
                    &source.body,
                    &source.metadata.vars,
                    supplied,
                    &mut context,
                )
                .with_context(|| format!("cannot render {:?}", source.name))
            })
            .collect()
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
