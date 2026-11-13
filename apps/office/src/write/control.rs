//! Write's tools on the control interface (`nyx_core::control`), for Nyx Assistant:
//! start, open, read, fill and save documents directly instead of typing into the window.
//! The tools work on the front document: the tab in front when it's a document, or the
//! document last in front. Starting or opening a document gives it a tab of its own, so
//! nothing here throws away unsaved work.

use super::model::Style;
use crate::app::{self, Kind, Tab};
use crate::{doc, print};
use nyx_core::control::{self, text};
use serde_json::{json, Value};

pub fn tools() -> Value {
    json!([
        {"name": "write.new_document", "description": "Start a new, empty Write document (in a tab of its own).", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "write.open", "description": "Open a document (inside the home folder) in Write, in a tab of its own.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
        {"name": "write.read", "description": "Read the front Write document: title, file and text (as Markdown).", "inputSchema": {"type": "object", "properties": {}}},
        {"name": "write.insert_text", "description": "Insert text into the front Write document at the cursor or the end; each line is a paragraph in the given style.", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}, "style": {"enum": ["body", "title", "heading1", "heading2", "heading3", "quote"]}, "at": {"enum": ["cursor", "end"]}}, "required": ["text"]}},
        {"name": "write.save", "description": "Save the front Write document; give a path (inside the home folder; .nwrite, .odt, .docx, .md…) for a new document or to save under a new name.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "overwrite": {"type": "boolean"}}}},
        {"name": "write.export_pdf", "description": "Export the front Write document as a PDF (a path inside the home folder ending in .pdf).", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "overwrite": {"type": "boolean"}}, "required": ["path"]}},
    ])
}

fn state() -> Value {
    json!({"title": doc::title(), "path": doc::path(), "unsaved": doc::dirty()})
}

/// The document the tools work on.
fn front() -> Result<Tab, String> {
    doc::front_of(Kind::Write).ok_or_else(|| "no document is open".to_string())
}

pub fn handle(name: &str, args: Value) -> Result<Value, String> {
    match name {
        "write.new_document" => {
            let tab = doc::new_of(Kind::Write, None);
            Ok(app::within(tab, || json!({"document": state()})))
        }
        "write.open" => {
            let path = control::home_path(&text(&args, "path")?)?;
            if !path.is_file() {
                return Err("no such file".into());
            }
            if Kind::for_path(&path) != Some(Kind::Write) {
                return Err("Write doesn't open that kind of file".into());
            }
            let tab = doc::open(path).ok_or("couldn't open it")?;
            Ok(app::within(tab, || json!({"document": state()})))
        }
        "write.read" => app::within(front()?, || {
            Ok(json!({"document": state(), "text": super::control_text().unwrap_or_default(), "untrusted_content": true}))
        }),
        "write.insert_text" => {
            let insert = text(&args, "text")?;
            if insert.is_empty() || insert.len() > 100_000 {
                return Err("text must be 1 to 100000 bytes".into());
            }
            let style = match args["style"].as_str().unwrap_or("body") {
                "body" => Style::Normal,
                "title" => Style::Title,
                "heading1" => Style::H1,
                "heading2" => Style::H2,
                "heading3" => Style::H3,
                "quote" => Style::Quote,
                other => return Err(format!("unknown style {other}")),
            };
            let tab = front().unwrap_or_else(|_| doc::new_of(Kind::Write, None));
            app::within(tab, || {
                super::control_insert(&insert, style, args["at"] == "end")?;
                Ok(json!({"document": state(), "inserted": insert.chars().count()}))
            })
        }
        "write.save" => app::within(front()?, || {
            let path = match args["path"].as_str() {
                Some(_) => control::save_target(&args)?,
                None => doc::path().ok_or("this document has no file yet; give a path")?,
            };
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !Kind::Write.save_formats().iter().any(|(_, x)| *x == ext) {
                let known: Vec<_> = Kind::Write.save_formats().iter().map(|(_, x)| format!(".{x}")).collect();
                return Err(format!("Write saves {}", known.join(", ")));
            }
            if !doc::save_to(&path) {
                return Err("couldn't save".into());
            }
            Ok(json!({"document": state(), "saved": path}))
        }),
        "write.export_pdf" => app::within(front()?, || {
            let path = control::save_target(&args)?;
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                return Err("the path must end in .pdf".into());
            }
            let ed = doc::editor().ok_or("no document is open")?;
            print::export_pdf(ed.pages().as_ref(), &path).map_err(|e| format!("{e:#}"))?;
            Ok(json!({"document": state(), "exported": path}))
        }),
        _ => Err(format!("unknown tool {name}")),
    }
}
