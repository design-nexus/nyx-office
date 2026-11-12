//! What saving a Word, Excel or PowerPoint file in place would lose: the file read and
//! written again with Office's own reader and writer, and the kinds of content the
//! original has and the rewrite doesn't. Measured, not listed by hand, so it follows
//! what the readers and writers actually keep.

use std::collections::{BTreeSet, HashMap};
use std::io::{Cursor, Read};
use std::path::Path;

/// A kind of content, by the XML elements (local names, in any part) or the package
/// parts (path fragments) that carry it.
struct Kind {
    name: &'static str,
    elements: &'static [&'static str],
    parts: &'static [&'static str],
}

const KINDS: &[Kind] = &[
    Kind { name: "text boxes", elements: &["txbxContent"], parts: &[] },
    Kind { name: "content controls", elements: &["sdt"], parts: &[] },
    Kind { name: "equations", elements: &["oMath", "oMathPara"], parts: &[] },
    Kind { name: "charts", elements: &[], parts: &["/charts/"] },
    Kind { name: "SmartArt diagrams", elements: &[], parts: &["/diagrams/"] },
    Kind { name: "embedded objects", elements: &["OLEObject", "oleObj"], parts: &["/embeddings/"] },
    Kind { name: "pictures", elements: &[], parts: &["/media/"] },
    Kind { name: "tracked changes", elements: &["ins", "del", "moveFrom", "moveTo"], parts: &[] },
    Kind { name: "comments", elements: &[], parts: &["comments", "/threadedComments/"] },
    Kind { name: "footnotes", elements: &["footnoteReference"], parts: &[] },
    Kind { name: "endnotes", elements: &["endnoteReference"], parts: &[] },
    Kind { name: "fields", elements: &["fldSimple", "instrText"], parts: &[] },
    Kind { name: "bookmarks", elements: &["bookmarkStart"], parts: &[] },
    Kind { name: "links", elements: &["hyperlink"], parts: &[] },
    Kind { name: "headers and footers", elements: &["headerReference", "footerReference", "headerFooter"], parts: &[] },
    Kind { name: "macros", elements: &[], parts: &["vbaProject.bin"] },
    Kind { name: "custom XML data", elements: &[], parts: &["customXml/"] },
    Kind { name: "pivot tables", elements: &[], parts: &["/pivotTables/"] },
    Kind { name: "data validation", elements: &["dataValidation"], parts: &[] },
    Kind { name: "conditional formatting", elements: &["conditionalFormatting"], parts: &[] },
    Kind { name: "merged cells", elements: &["mergeCell"], parts: &[] },
    Kind { name: "named ranges", elements: &["definedName"], parts: &[] },
    Kind { name: "speaker notes", elements: &[], parts: &["/notesSlides/"] },
    Kind { name: "animations", elements: &["timing"], parts: &[] },
    Kind { name: "slide transitions", elements: &["transition"], parts: &[] },
];

/// What's in a package: its part names and how often each element occurs.
#[derive(Default)]
struct Contents {
    parts: Vec<String>,
    elements: HashMap<String, usize>,
}

fn contents(bytes: &[u8]) -> Option<Contents> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut c = Contents::default();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).ok()?;
        let name = f.name().to_string();
        if name.ends_with(".xml") && f.size() < 64 << 20 {
            let mut text = String::new();
            if f.read_to_string(&mut text).is_ok()
                && let Ok(d) = crate::xml::parse(&text)
            {
                for n in d.descendants().filter(|n| n.is_element()) {
                    *c.elements.entry(n.tag_name().name().to_string()).or_default() += 1;
                }
            }
        }
        c.parts.push(name);
    }
    Some(c)
}

impl Contents {
    fn has(&self, k: &Kind) -> bool {
        k.elements.iter().any(|e| self.elements.get(*e).is_some_and(|n| *n > 0))
            || k.parts.iter().any(|p| self.parts.iter().any(|name| name.contains(p)))
    }

    /// Sections with their own page setup (Word): more than one `sectPr`.
    fn sections(&self) -> usize {
        self.elements.get("sectPr").copied().unwrap_or(0)
    }
}

/// The kinds of content in `before` that aren't in `after`, in KINDS order.
fn lost(before: &[u8], after: &[u8]) -> Vec<String> {
    let (Some(b), Some(a)) = (contents(before), contents(after)) else { return Vec::new() };
    let mut out: Vec<String> = KINDS.iter().filter(|k| b.has(k) && !a.has(k)).map(|k| k.name.to_string()).collect();
    if b.sections() > 1 && a.sections() < b.sections() {
        out.push("sections with their own page setup".into());
    }
    let mut seen = BTreeSet::new();
    out.retain(|k| seen.insert(k.clone()));
    out
}

/// What saving `bytes` (a file with `ext`) in place would lose; empty for formats this
/// doesn't check, or when the file can't be read.
pub fn unkept(ext: &str, bytes: &[u8]) -> Vec<String> {
    let after = match ext {
        "docx" => crate::write::docx_round_trip(bytes),
        "xlsx" => crate::sheets::xlsx_round_trip(bytes),
        "pptx" => crate::present::pptx_round_trip(bytes),
        _ => return Vec::new(),
    };
    after.map(|a| lost(bytes, &a)).unwrap_or_default()
}

/// `unkept` for the file at `path`.
pub fn unkept_in(path: &Path) -> Vec<String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match std::fs::read(path) {
        Ok(bytes) => unkept(&ext, &bytes),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal Word document whose body is `body` (WordprocessingML).
    fn docx(body: &str) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let o = zip::write::SimpleFileOptions::default();
        let parts = [
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_string(),
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_string(),
            ),
            (
                "word/document.xml",
                format!(
                    r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><w:body>{body}</w:body></w:document>"#
                ),
            ),
        ];
        for (name, text) in parts {
            zip.start_file(name, o).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    const PARA: &str = "<w:p><w:r><w:t>Plain words.</w:t></w:r></w:p>";

    #[test]
    fn a_plain_document_loses_nothing() {
        let body = format!(
            "{PARA}<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Title</w:t></w:r></w:p>"
        );
        assert_eq!(unkept("docx", &docx(&body)), Vec::<String>::new());
    }

    #[test]
    fn text_boxes_controls_and_equations_are_named() {
        let body = format!(
            "{PARA}\
             <w:p><w:r><w:pict><v:shape><v:textbox><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>\
             <w:sdt><w:sdtContent><w:p><w:r><w:t>Controlled</w:t></w:r></w:p></w:sdtContent></w:sdt>\
             <w:p><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara></w:p>"
        );
        let lost = unkept("docx", &docx(&body));
        for k in ["text boxes", "content controls", "equations"] {
            assert!(lost.iter().any(|l| l == k), "{k} not reported: {lost:?}");
        }
    }

    #[test]
    fn other_formats_and_unreadable_files_are_left_alone() {
        assert!(unkept("odt", b"anything").is_empty());
        assert!(unkept("docx", b"not a zip").is_empty());
    }

    #[test]
    fn content_present_after_saving_is_not_reported() {
        let before = docx(&format!("{PARA}<w:sdt><w:sdtContent>{PARA}</w:sdtContent></w:sdt>"));
        assert_eq!(lost(&before, &before), Vec::<String>::new());
    }

    /// Office's own files lose nothing to Office: run over the samples
    /// (`NXO_SAMPLE_DIR=… cargo test -- --ignored write_office_samples` first) or any
    /// folder of Office files written by Office:
    /// `NXO_SAMPLE_DIR=… cargo test -- --ignored office_files_lose_nothing --nocapture`.
    #[test]
    #[ignore]
    fn office_files_lose_nothing() {
        let Some(dir) = std::env::var_os("NXO_SAMPLE_DIR").map(std::path::PathBuf::from) else { return };
        let mut checked = 0;
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if !["docx", "xlsx", "pptx"].contains(&ext.as_str()) {
                continue;
            }
            let lost = unkept_in(&p);
            println!("{}: {lost:?}", p.display());
            assert!(lost.is_empty(), "{} loses {lost:?}", p.display());
            checked += 1;
        }
        assert!(checked > 0, "no Office files in {}", dir.display());
    }
}
