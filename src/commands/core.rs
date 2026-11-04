//! The local CLI journey. Rendering finishes before any destination is touched.
use super::common::{fail, parse_variables, read_input, resolve_prompt_name, usage, write_stdout};
use crate::{
    cli::Commands, compose::is_explicit_file, matching, Composer, Importer, IoOptions, Prompt,
    PromptMetadata, Storage,
};
use anyhow::{Context, Result};
use std::io::IsTerminal;
use std::time::Instant;

fn interactive() -> bool {
    #[cfg(feature = "tui")]
    {
        crate::tui::interactive_supported() && std::io::stdout().is_terminal()
    }
    #[cfg(not(feature = "tui"))]
    {
        false
    }
}
fn select(storage: &Storage, query: &str) -> Result<String> {
    if !interactive() {
        return Err(fail(2,"search preview needs a capable terminal; use ph f QUERY, then ph u EXACT_NAME (or -q for text)"));
    }
    #[cfg(feature = "tui")]
    {
        crate::tui::pick_prompt(storage, query)?
            .ok_or_else(|| fail(130, "Cancelled; no prompt was used."))
    }
    #[cfg(not(feature = "tui"))]
    {
        let _ = (storage, query);
        unreachable!()
    }
}
fn select_or_resolve(storage: &Storage, name: Option<&str>) -> Result<String> {
    let Some(name) = name else {
        return select(storage, "");
    };
    match resolve_prompt_name(storage, name) {
        Ok(name) => Ok(name),
        Err(e) if interactive() && super::common::error_code(&e) == 2 => select(storage, name),
        Err(e) => Err(e),
    }
}
/// Only an actual picker journey may ask for missing values. Exact scripts stay strict.
fn render_use(
    storage: &Storage,
    requested: Option<&str>,
    directives: Option<&str>,
    input: &str,
    variables: &crate::template::Variables,
    destination: impl Fn(&str) -> String,
    force_picker: bool,
) -> Result<(String, String)> {
    let prefix = directives.map(|text| sources(storage, text)).transpose()?;
    let composer = Composer::new(storage.clone());
    let render = |name: &str, values: &crate::template::Variables| {
        match &prefix {
            Some(names) => composer.render_with_directives(name, names, input, values),
            None => composer.render(&[name.to_owned()], input, values),
        }
        .map_err(|error| fail(2, format!("{error:#}")))
    };
    let query = if force_picker {
        requested.unwrap_or("")
    } else if let Some(name) = requested {
        match resolve_prompt_name(storage, name) {
            Ok(name) => {
                let text = render(&name, variables)?;
                return Ok((name, text));
            }
            Err(error) if interactive() && super::common::error_code(&error) == 2 => name,
            Err(error) => return Err(error),
        }
    } else {
        ""
    };
    if !interactive() {
        return Err(fail(2,"search preview needs a capable terminal; use ph f QUERY, then ph u EXACT_NAME (or -q for text)"));
    }
    #[cfg(feature = "tui")]
    {
        crate::tui::pick_rendered(
            storage,
            query,
            variables,
            destination,
            |name, values| {
                let mut names = prefix.clone().unwrap_or_default();
                names.push(name.to_owned());
                composer
                    .required_variables(&names, values)
                    .map_err(|error| fail(2, format!("{error:#}")))
            },
            render,
        )?
        .ok_or_else(|| fail(130, "Cancelled; no prompt was used."))
    }
    #[cfg(not(feature = "tui"))]
    {
        let _ = (query, destination);
        unreachable!("interactive() is false without the tui feature")
    }
}
fn sources(storage: &Storage, text: &str) -> Result<Vec<String>> {
    let names = crate::parse_prompt_list(text);
    if names.is_empty() {
        return Err(fail(2, "supply at least one prompt name"));
    }
    names
        .iter()
        .map(|name| {
            if name.is_empty() {
                Err(fail(
                    2,
                    "empty composition stage; use comma-separated names",
                ))
            } else if is_explicit_file(name) {
                Ok(name.clone())
            } else {
                select_or_resolve(storage, Some(name))
            }
        })
        .collect()
}
fn terminal_description(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

#[cfg(feature = "tui")]
fn safe_terminal_body(text: &str) -> Result<String> {
    let mut output = String::new();
    for character in text.chars() {
        if character.is_control() && !matches!(character, '\n' | '\t') {
            for escaped in character.escape_default() {
                let mut bytes = [0; 4];
                crate::limits::push_str(
                    &mut output,
                    escaped.encode_utf8(&mut bytes),
                    "shown text",
                )?;
            }
        } else {
            let mut bytes = [0; 4];
            crate::limits::push_str(&mut output, character.encode_utf8(&mut bytes), "shown text")?;
        }
    }
    Ok(output)
}
fn readable_prompts(storage: &Storage) -> Result<Vec<Prompt>> {
    let mut prompts = Vec::new();
    let mut budget = crate::limits::Budget::new(crate::limits::IMPORT_BYTES, "prompt search index");
    for name in storage.list_prompts()? {
        let (meta, _) = storage.read_prompt(&name)?;
        budget.add(name.len())?;
        budget.add(meta.description.len())?;
        budget.add(std::mem::size_of::<Prompt>())?;
        prompts.push(Prompt {
            name: name.clone(),
            short_code: String::new(),
            description: meta.description,
            // Discovery uses names and descriptions only. Do not retain and
            // clone arbitrary legacy metadata for every search keystroke.
            version: None,
            created_at: None,
            updated_at: None,
            git_hash: None,
        });
    }
    Ok(prompts)
}
fn listing(prompts: &[Prompt], descriptions: bool) -> Result<String> {
    let mut text = String::new();
    for prompt in prompts {
        crate::limits::push_str(&mut text, &prompt.name, "prompt listing")?;
        if descriptions {
            crate::limits::push_str(&mut text, "  ", "prompt listing")?;
            crate::limits::push_str(
                &mut text,
                &terminal_description(&prompt.description),
                "prompt listing",
            )?;
        }
        crate::limits::push_str(&mut text, "\n", "prompt listing")?;
    }
    Ok(text)
}
pub fn run(command: Option<Commands>) -> Result<()> {
    let start = Instant::now();
    if command.is_none() {
        let input = read_input(None)?;
        if !interactive() {
            if input.is_empty() {
                write_stdout("ph — local prompts and notes\n\n  u use      render a known prompt\n  f find     search names and descriptions\n  n new      save text\n  e edit     edit saved text\n  c compose  chain left to right\n\n  ph n review 'Review {input}'\n  git diff | ph u review\n\nUse copies in a terminal (shows text if unavailable); pipes print text.\nRun ph in a terminal to search and preview, or ph --help for all actions.\n")?;
                return Ok(());
            }
            return Err(fail(
                2,
                "piped context needs an exact prompt: ph u NAME or ph c FIRST,SECOND",
            ));
        }
        let storage = Storage::new()?;
        storage.init()?;
        #[cfg(feature = "tui")]
        {
            use crate::tui::BareSelection;
            return match crate::tui::pick_bare(&storage, &input)?
                .ok_or_else(|| fail(130, "Cancelled; no prompt was used."))?
            {
                BareSelection::Use(name, text) => {
                    IoOptions::default().emit(&storage, &text, &name, None)
                }
                BareSelection::Show(name) => {
                    let (_, text) = storage.read_prompt(&name)?;
                    let safe = safe_terminal_body(&text)?;
                    write_stdout(&safe)
                }
                BareSelection::Edit(name) => {
                    crate::edit::edit_prompt(&storage, &name)?;
                    eprintln!("Saved {name}.");
                    Ok(())
                }
            };
        }
        #[cfg(not(feature = "tui"))]
        unreachable!("interactive() is false without the tui feature");
    }
    // Help/completions do not create or inspect a store.
    if let Some(Commands::Completion { shell }) = command {
        return super::completion::handle_completion(&shell).map_err(usage);
    }
    if matches!(command, Some(Commands::Update)) {
        return super::update::run();
    }
    let storage = Storage::new()?;
    match command.expect("checked above") {
        Commands::Use{name,input,edit,vars,with_directives,clipboard,destination}=>{
            let io=IoOptions::new(destination,clipboard)?;
            let variables=parse_variables(&vars)?;
            let input=read_input(input.as_deref())?;
            storage.init()?;
            let (name,mut text)=render_use(&storage,name.as_deref(),with_directives.as_deref(),&input,&variables,|name| { let target=io.confirmation(name); if edit { format!("Review in EDITOR, then {target}") } else { target } },false)?;
            if edit {text=crate::edit::edit_content(&text)?;}
            io.emit(&storage,&text,&name,None)
        }
        Commands::Show{name,edit,clipboard,destination}=>{
            let io=IoOptions::new(destination,clipboard)?;storage.init()?;
            let name=select_or_resolve(&storage,Some(&name))?;
            if edit {crate::edit::edit_prompt(&storage,&name)?;}
            let (metadata,text)=storage.read_prompt(&name)?;
            io.emit(&storage,&text,&name,Some(&metadata))
        }
        Commands::Compose{prompts,input,edit,vars,clipboard,destination}=>{
            let io=IoOptions::new(destination,clipboard)?;
            let variables=parse_variables(&vars)?;let input=read_input(input.as_deref())?;
            storage.init()?;let names=sources(&storage,&prompts)?;
            let mut text=Composer::new(storage.clone()).render(&names,&input,&variables).map_err(|e|fail(2,format!("{e:#}")))?;
            if edit {text=crate::edit::edit_content(&text)?;}
            io.emit(&storage,&text,"composition",None)
        }
        Commands::New{name,content,edit,clean,clipboard,destination}=>{
            let io=IoOptions::new(destination,clipboard)?;
            let clearly_content=name.chars().any(|c|c.is_whitespace() || matches!(c,'{'|'}'|'?'));
            let (name,argument)=match content {
                Some(content) if Storage::validate_name(&name).is_ok() => (name,Some(content)),
                Some(content) if clearly_content && Storage::validate_name(&content).is_ok() => (content,Some(name)),
                Some(_)=>return Err(fail(2,"invalid prompt name; use ph n NAME 'content'")),
                None=>(name,None),
            };
            Storage::validate_name(&name).map_err(usage)?;
            let auto_edit=argument.is_none() && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
            let mut text=read_input(argument.as_deref())?;
            if clean {text=crate::clean::clean_text(&text);}
            if edit || auto_edit {
                io.preflight_new(&storage,&name)?;
                let initially_empty=text.is_empty() && argument.is_none();
                text=crate::edit::edit_content(&text)?;
                if initially_empty && text.is_empty() { return Err(fail(130,"Editor cancelled; no empty note was created. Supply an explicit empty argument to create one.")); }
            }
            let metadata=PromptMetadata{id:name.clone(),..Default::default()};
            // One explicit destination replaces the default save. A failed
            // export or copy never leaves a surprising extra saved prompt.
            if io.clipboard || io.destination.save.is_some() || io.destination.append.is_some() || io.destination.file.is_some() {
                io.emit(&storage,&text,&name,Some(&metadata))
            } else {
                if storage.prompt_exists(&name) {return Err(fail(2,format!("prompt {name:?} already exists; use ph e to edit")));}
                storage.create_prompt(&name,&metadata,&text)?;
                io.status(&format!("Saved {name}."));Ok(())
            }
        }
        Commands::Edit{name}=>{
            let name=if let Some(name)=name { storage.init()?;resolve_prompt_name(&storage,&name)? } else {
                if !std::io::stdin().is_terminal() || !interactive() { return Err(fail(2,"ph e needs an exact NAME in pipes; run ph e in a terminal to search.")); }
                storage.init()?;
                #[cfg(feature="tui")]
                { crate::tui::pick_prompt_for(&storage,"",crate::tui::PickerIntent::Edit)?.ok_or_else(||fail(130,"Cancelled; no prompt was edited."))? }
                #[cfg(not(feature="tui"))]
                { unreachable!("interactive() is false without tui") }
            };
            crate::edit::edit_prompt(&storage,&name)?;eprintln!("Saved {name}.");Ok(())
        }
        Commands::Ls{clipboard,destination}=>{
            let io=IoOptions::new(destination,clipboard)?;
            let explicit=io.clipboard || io.destination.save.is_some() || io.destination.append.is_some() || io.destination.file.is_some();
            let text=if std::io::stdout().is_terminal() && !explicit && !io.destination.quiet {
                listing(&readable_prompts(&storage)?,true)?
            } else {
                let mut text=String::new();
                for name in storage.list_prompts()? { crate::limits::push_str(&mut text,&name,"prompt listing")?;crate::limits::push_str(&mut text,"\n","prompt listing")?; }
                text
            };
            if explicit {io.emit(&storage,&text,"prompts",None)} else {write_stdout(&text)}
        }
        Commands::Find{query,clipboard,destination}=>{
            let io=IoOptions::new(destination,clipboard)?;
            let explicit=io.clipboard || io.destination.save.is_some() || io.destination.append.is_some() || io.destination.file.is_some();
            let prompts=matching::ranked_prompts(&query,&readable_prompts(&storage)?);
            let text=listing(&prompts,std::io::stdout().is_terminal() && !explicit && !io.destination.quiet)?;
            if explicit {io.emit(&storage,&text,"matches",None)} else {write_stdout(&text)}
        }
        Commands::Rename{old_name,new_name}=>{
            Storage::validate_name(&new_name).map_err(usage)?;
            let old=resolve_prompt_name(&storage,&old_name)?;
            if storage.prompt_exists(&new_name){return Err(fail(2,"rename target already exists"));}
            storage.rename_prompt(&old,&new_name)?;eprintln!("Renamed {old} to {new_name}; previous content was backed up.");Ok(())
        }
        Commands::Delete{name}=>{
            let name=resolve_prompt_name(&storage,&name)?;storage.delete_prompt(&name)?;
            eprintln!("Deleted {name}; saved versions remain available.");Ok(())
        }
        Commands::Clean{text,clipboard,destination}=>{
            let io=IoOptions::new(destination,clipboard)?;
            let text=read_input(text.as_deref())?;
            io.emit(&storage,&crate::clean::clean_text(&text),"cleaned",None)
        }
        Commands::Tui{search}=>{
            let input=read_input(None)?;
            if !interactive(){return Err(fail(2,"terminal preview is unavailable; use ph f QUERY then ph u EXACT_NAME"));}
            storage.init()?;
            // t is an explicit discovery request even when SEARCH is an exact name.
            let (name,text)=render_use(&storage,search.as_deref(),None,&input,&Default::default(),|name|IoOptions::default().confirmation(name),true)?;
            IoOptions::default().emit(&storage,&text,&name,None)
        }
        Commands::Export{path}=>{
            let count=crate::portability::export_store(&storage,std::path::Path::new(&path))?;
            eprintln!("Exported {count} prompts to {path}.");Ok(())
        }
        Commands::Import{path,name,force,version,skip,update,dry_run}=>{
            let result=Importer::new(storage).import_with_options(&path,name.as_deref(),force,version,skip,update,dry_run)?;
            result.display();if result.has_errors(){return Err(fail(1,"import reported errors; inspect the paths above before retrying"));}Ok(())
        }
        Commands::Diff{prompt1,prompt2,format,context,output,force}=>{
            if !matches!(format.as_str(),"unified"|"side-by-side"|"side"|"brief"){return Err(fail(2,"diff format must be unified, side-by-side, or brief"));}
            let a=resolve_prompt_name(&storage,&prompt1)?;let b=resolve_prompt_name(&storage,&prompt2)?;
            super::diff_merge::handle_diff_with_force(&storage,&a,&b,&format,context,output.as_deref(),force,start)
        }
        Commands::Merge{source,target,backup,preview,interactive}=>{
            if interactive{return Err(fail(2,"interactive merge is unsupported; use --preview before replacing an exact target"));}
            let source=resolve_prompt_name(&storage,&source)?;let target=resolve_prompt_name(&storage,&target)?;
            super::diff_merge::handle_merge(&storage,&source,&target,backup,preview,false,start)
        }
        Commands::Version{name,tag,message}=>{
            Storage::validate_component(&tag).map_err(usage)?;let name=resolve_prompt_name(&storage,&name)?;
            super::versioning::handle_version(&storage,&name,&tag,message.as_deref(),start)
        }
        Commands::Versions{name,verbose}=>{
            Storage::validate_name(&name).map_err(usage)?;
            super::versioning::handle_versions(&storage,&name,verbose,start)
        }
        Commands::Rollback{name,version,backup}=>{
            Storage::validate_name(&name).map_err(usage)?;Storage::validate_component(&version).map_err(usage)?;
            super::versioning::handle_rollback(&storage,&name,&version,backup,start)
        }
        Commands::Completion{..}|Commands::Update=>unreachable!(),
    }.context("command failed")
}

#[cfg(all(test, feature = "tui"))]
mod terminal_body_tests {
    use super::safe_terminal_body;

    #[test]
    fn show_escapes_controls_and_bounds_expansion() {
        assert_eq!(
            safe_terminal_body("hello\n\x1b]52;\x07").unwrap(),
            "hello\n\\u{1b}]52;\\u{7}"
        );
        let controls = "\x1b".repeat(crate::limits::TEXT_BYTES / 6 + 1);
        assert!(safe_terminal_body(&controls).is_err());
    }
}
