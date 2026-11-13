//! Documents, spreadsheets and presentations: the three kinds of file the one Office
//! window opens, each file in its own tab. `tab()` is the tab being worked on: the one in front,
//! or the one a background job (an autosave, Nyx Assistant) works on through
//! `within`; `kind()` is its kind (with no tab, the kind last worked on).

use std::cell::Cell;
use std::path::Path;

/// Office's application id: one window for all three kinds.
pub const APP_ID: &str = "io.github.design_nexus.nyx.Office";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Write,
    Sheets,
    Present,
}

/// An open file's tab, with its own editor and file state. Ids aren't reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tab {
    pub id: u32,
    pub kind: Kind,
}

impl Tab {
    /// The name of its page in the window.
    pub fn page(self) -> String {
        format!("tab-{}", self.id)
    }

    /// The tab a page name (`tab-3`) stands for, by its id.
    pub fn id_of_page(page: &str) -> Option<u32> {
        page.strip_prefix("tab-")?.parse().ok()
    }
}

thread_local! {
    static KIND: Cell<Kind> = const { Cell::new(Kind::Write) };
    static TAB: Cell<Option<Tab>> = const { Cell::new(None) };
    /// The document tab in front; None while Home (or Recent, or Templates) is.
    static FRONT: Cell<Option<Tab>> = const { Cell::new(None) };
    static NEXT: Cell<u32> = const { Cell::new(1) };
}

/// A new tab's id, for a file of `kind`.
pub fn new_tab(kind: Kind) -> Tab {
    let id = NEXT.with(|n| n.replace(n.get() + 1));
    Tab { id, kind }
}

fn set(tab: Option<Tab>) {
    TAB.with(|t| t.set(tab));
    if let Some(t) = tab {
        KIND.with(|k| k.set(t.kind));
    }
}

/// The tab being worked on.
pub fn tab() -> Option<Tab> {
    TAB.with(|t| t.get())
}

pub fn kind() -> Kind {
    tab().map_or_else(|| KIND.with(|k| k.get()), |t| t.kind)
}

/// The document tab in front, if one is.
pub fn front() -> Option<Tab> {
    FRONT.with(|f| f.get())
}

pub fn set_front(front: Option<Tab>) {
    FRONT.with(|f| f.set(front));
    set(front);
}

/// Run `f` working on `tab`, then go back to what was being worked on, or to the
/// tab `f` brought to the front.
pub fn within<R>(tab: Tab, f: impl FnOnce() -> R) -> R {
    let (before, kind_before, front_before) = (self::tab(), KIND.with(|k| k.get()), front());
    set(Some(tab));
    let out = f();
    let now = front();
    let back = if now != front_before { now } else { before };
    TAB.with(|t| t.set(back));
    KIND.with(|k| k.set(back.map_or(kind_before, |t| t.kind)));
    out
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Write, Kind::Sheets, Kind::Present];

    pub fn parse(id: &str) -> Option<Kind> {
        match id.trim_start_matches("nyx-").trim_start_matches("nixe-").trim_start_matches("nexus-") {
            "write" => Some(Kind::Write),
            "sheets" => Some(Kind::Sheets),
            "present" => Some(Kind::Present),
            _ => None,
        }
    }

    /// The kind that opens `path`, by its extension.
    pub fn for_path(path: &Path) -> Option<Kind> {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        Kind::ALL.into_iter().find(|k| k.opens().contains(&ext.as_str()))
    }

    /// 0, 1, 2: for per-kind tables.
    pub fn index(self) -> usize {
        match self {
            Kind::Write => 0,
            Kind::Sheets => 1,
            Kind::Present => 2,
        }
    }

    /// `write`, `sheets` or `present`: used in file and folder names.
    pub fn id(self) -> &'static str {
        match self {
            Kind::Write => "write",
            Kind::Sheets => "sheets",
            Kind::Present => "present",
        }
    }

    /// The id the app had when each kind ran on its own. Write's control interface
    /// still answers there, and the icons keep these names.
    pub fn app_id(self) -> &'static str {
        match self {
            Kind::Write => "io.github.design_nexus.nyx.Write",
            Kind::Sheets => "io.github.design_nexus.nyx.Sheets",
            Kind::Present => "io.github.design_nexus.nyx.Present",
        }
    }

    /// "document", "spreadsheet", "presentation".
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Write => "document",
            Kind::Sheets => "spreadsheet",
            Kind::Present => "presentation",
        }
    }

    /// "Document", "Spreadsheet", "Presentation": the New cards and menu.
    pub fn title(self) -> &'static str {
        match self {
            Kind::Write => "Document",
            Kind::Sheets => "Spreadsheet",
            Kind::Present => "Presentation",
        }
    }

    /// "Documents", "Spreadsheets", "Presentations": the sidebar.
    pub fn plural(self) -> &'static str {
        match self {
            Kind::Write => "Documents",
            Kind::Sheets => "Spreadsheets",
            Kind::Present => "Presentations",
        }
    }

    /// The recent files' filter: "Documents", "Sheets", "Slides".
    pub fn filter_label(self) -> &'static str {
        match self {
            Kind::Write => "Documents",
            Kind::Sheets => "Sheets",
            Kind::Present => "Slides",
        }
    }

    /// The theme swatch each kind is shown in (`.section-swatch-<name>` colours it).
    pub fn swatch(self) -> &'static str {
        match self {
            Kind::Write => "blue",
            Kind::Sheets => "green",
            Kind::Present => "coral",
        }
    }

    /// The CSS class that colours this kind's icons.
    pub fn color_class(self) -> String {
        format!("section-swatch-{}", self.swatch())
    }

    /// The native file extension, without the dot.
    pub fn native_ext(self) -> &'static str {
        match self {
            Kind::Write => "nwrite",
            Kind::Sheets => "nsheets",
            Kind::Present => "npresent",
        }
    }

    /// Every extension this kind opens.
    pub fn opens(self) -> &'static [&'static str] {
        match self {
            Kind::Write => &["nwrite", "docx", "odt", "md", "markdown", "txt"],
            Kind::Sheets => &["nsheets", "xlsx", "ods", "csv"],
            Kind::Present => &["npresent", "pptx", "odp"],
        }
    }

    /// The icon for files of this kind.
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Write => "nxo-file-text-symbolic",
            Kind::Sheets => "nxo-file-spreadsheet-symbolic",
            Kind::Present => "nxo-presentation-symbolic",
        }
    }

    /// (name, extension) for each format the Save As dialog offers, native first.
    pub fn save_formats(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Kind::Write => &[
                ("Office document", "nwrite"),
                ("Word document", "docx"),
                ("OpenDocument text", "odt"),
                ("Markdown", "md"),
                ("Plain text", "txt"),
            ],
            Kind::Sheets => {
                &[
                    ("Office workbook", "nsheets"),
                    ("Excel workbook", "xlsx"),
                    ("OpenDocument spreadsheet", "ods"),
                    ("CSV (one sheet, values only)", "csv"),
                ]
            }
            Kind::Present => {
                &[("Office slides", "npresent"), ("PowerPoint slides", "pptx"), ("OpenDocument presentation", "odp")]
            }
        }
    }
}

/// True when Office opens `path` (any of the three kinds).
pub fn opens_any(path: &Path) -> bool {
    Kind::for_path(path).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_go_to_their_kind() {
        assert_eq!(Kind::for_path(Path::new("a/Budget.XLSX")), Some(Kind::Sheets));
        assert_eq!(Kind::for_path(Path::new("talk.odp")), Some(Kind::Present));
        assert_eq!(Kind::for_path(Path::new("notes.md")), Some(Kind::Write));
        assert_eq!(Kind::for_path(Path::new("photo.png")), None);
    }

    #[test]
    fn within_comes_back_to_the_front() {
        let (sheet, letter) = (new_tab(Kind::Sheets), new_tab(Kind::Write));
        set_front(Some(sheet));
        assert_eq!(within(letter, kind), Kind::Write);
        assert_eq!(within(letter, tab), Some(letter));
        assert_eq!(tab(), Some(sheet));
        set_front(None);
        assert_eq!(kind(), Kind::Sheets);
        within(letter, || {});
        assert_eq!((tab(), kind()), (None, Kind::Sheets));
    }

    #[test]
    fn within_nests() {
        let (a, b, c, d) = (new_tab(Kind::Write), new_tab(Kind::Write), new_tab(Kind::Sheets), new_tab(Kind::Present));
        assert_ne!(a, b);
        set_front(Some(a));
        within(b, || {
            within(c, || {});
            assert_eq!(tab(), Some(b));
            within(d, || {});
            assert_eq!(tab(), Some(b));
        });
        assert_eq!(tab(), Some(a));
        // A file brought to the front inside stays what's worked on.
        within(d, || set_front(Some(c)));
        assert_eq!(tab(), Some(c));
        // Going to Home inside leaves no tab being worked on.
        within(d, || set_front(None));
        assert_eq!(tab(), None);
    }

    #[test]
    fn pages_name_their_tab() {
        let t = new_tab(Kind::Present);
        assert_eq!(Tab::id_of_page(&t.page()), Some(t.id));
        assert_eq!(Tab::id_of_page("home"), None);
    }
}
