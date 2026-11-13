//! The open files, each in its own tab with its own editor: any number of
//! documents, spreadsheets and presentations at once. Each kind (Write, Sheets,
//! Present) supplies an `Editor`; this module owns everything around it: each
//! file's path and saved state, opening and saving (native or Office format,
//! chosen by extension), the unsaved-changes guard, crash recovery, PDF export
//! and printing.
//!
//! Everything here works on `app::tab()`. Work that finishes later (a dialog's
//! answer, an autosave, a background check) goes back to the tab it started on
//! through `app::within`, whichever tab is in front by then.

use crate::app::{self, Kind, Tab};
use crate::print::Pages;
use crate::{cmd, paths, prefs, print, recent, widgets, window};
use anyhow::Result;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct Template {
    pub id: &'static str,
    pub title: &'static str,
    /// The template's name on Home's New cards ("Letter", "Budget").
    pub short: &'static str,
    pub summary: &'static str,
    pub icon: &'static str,
}

/// A group the editor adds to the sidebar (the outline, the slides).
pub struct SidebarGroup {
    pub title: String,
    /// A flat `+` at the right of the group label.
    pub add: Option<(String, Rc<dyn Fn()>)>,
    pub widget: gtk::Widget,
}

/// Find and replace inside the open file.
pub trait Find {
    /// Select the next (or previous) match after the cursor. False when there's none.
    fn find(&self, query: &str, case: bool, forward: bool) -> bool;
    /// Replace the selected match, then select the next one.
    fn replace(&self, query: &str, with: &str, case: bool) -> bool;
    fn replace_all(&self, query: &str, with: &str, case: bool) -> usize;
    fn count(&self, query: &str, case: bool) -> usize;
    /// The find bar closed: drop any match highlights.
    fn done(&self) {}
}

pub type Shortcut = (&'static [&'static str], &'static str);
pub type Action = Rc<dyn Fn()>;
/// A row for the file menu: its label and what it does.
pub type MenuItem = (&'static str, Action);
type Then = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

pub trait Editor {
    /// The document page: its toolbar row and the editor under it.
    fn widget(&self) -> gtk::Widget;
    fn sidebar(&self) -> Option<SidebarGroup> {
        None
    }
    /// Start a blank file, or one from a template.
    fn new_document(&self, template: Option<&str>);
    fn open(&self, path: &Path) -> Result<()>;
    /// Write the file in the format its extension names. Returns what that format
    /// couldn't keep ("speaker notes", "charts"), for a one-time note.
    fn save(&self, path: &Path) -> Result<Vec<String>>;
    /// The whole file in the native format, for crash recovery.
    fn snapshot(&self) -> Result<Vec<u8>>;
    /// Load a recovery snapshot.
    fn restore(&self, bytes: &[u8]) -> Result<()>;
    fn undo(&self);
    fn redo(&self);
    /// The readout in the header.
    fn status(&self) -> String;
    /// The editor shows its readout itself (Sheets, beside its sheet tabs).
    fn own_readout(&self) -> bool {
        false
    }
    fn find(&self) -> Option<Rc<dyn Find>> {
        None
    }
    fn pages(&self) -> Rc<dyn Pages>;
    fn focus(&self) {}

    /// A key the window didn't use. True when the editor handled it.
    fn key(&self, _key: gtk::gdk::Key, _mods: gtk::gdk::ModifierType) -> bool {
        false
    }

    /// Show the document larger or smaller (1.0 is actual size).
    fn set_zoom(&self, _zoom: f64) {}
}

/// A new editor of `kind`. Built inside the tab it's for (`app::within`), so it
/// registers its header tools and Inspector there and reports its edits for it.
pub fn build_editor(kind: Kind) -> Rc<dyn Editor> {
    match kind {
        Kind::Write => crate::write::Writer::create(),
        Kind::Sheets => crate::sheets::Sheets::create(),
        Kind::Present => crate::present::Present::create(),
    }
}

/// The templates a kind starts files from.
pub fn templates(kind: Kind) -> &'static [Template] {
    match kind {
        Kind::Write => crate::write::TEMPLATES,
        Kind::Sheets => crate::sheets::TEMPLATES,
        Kind::Present => crate::present::TEMPLATES,
    }
}

/// A kind's own shortcuts, for the F1 list.
pub fn shortcuts(kind: Kind) -> &'static [Shortcut] {
    match kind {
        Kind::Write => crate::write::SHORTCUTS,
        Kind::Sheets => crate::sheets::SHORTCUTS,
        Kind::Present => crate::present::SHORTCUTS,
    }
}

/// `f`, run later on the tab being worked on now.
fn here(f: impl FnOnce() + 'static) -> impl FnOnce() + 'static {
    let tab = app::tab();
    move || match tab {
        Some(t) => app::within(t, f),
        None => f(),
    }
}

// ---------- Zoom ----------

/// The zoom levels the buttons and keys step through.
pub const ZOOMS: [f64; 13] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

/// The zoom last set for a kind, which its new tabs start at. It's kept in its own
/// file in the state folder (not the settings, which every kind shares).
fn kind_zoom(kind: Kind) -> f64 {
    std::fs::read_to_string(paths::zoom_file(kind.id()))
        .ok()
        .and_then(|t| t.trim().parse::<f64>().ok())
        .filter(|z| (ZOOMS[0]..=ZOOMS[ZOOMS.len() - 1]).contains(z))
        .unwrap_or(1.0)
}

/// The zoom of the tab being worked on.
pub fn zoom() -> f64 {
    zoom_of(app::tab())
}

/// A tab's zoom (with no tab, the kind's).
pub fn zoom_of(tab: Option<Tab>) -> f64 {
    let Some(tab) = tab else { return kind_zoom(app::kind()) };
    if let Some(z) = with(tab, |d| d.zoom).flatten() {
        return z;
    }
    let z = kind_zoom(tab.kind);
    with(tab, |d| d.zoom = Some(z));
    z
}

/// Zoom the tab being worked on to `z` (clamped to the levels' range); its kind's
/// next tabs start there.
pub fn set_zoom(z: f64) {
    let Some(tab) = app::tab() else { return };
    let z = (z.clamp(ZOOMS[0], ZOOMS[ZOOMS.len() - 1]) * 100.0).round() / 100.0;
    with(tab, |d| d.zoom = Some(z));
    let _ = cmd::atomic_write(&paths::zoom_file(tab.kind.id()), format!("{z}\n"));
    if let Some(e) = editor() {
        e.set_zoom(z);
    }
    emit(Change::Status);
}

/// One level in (`up`) or out from the current zoom.
pub fn zoom_step(up: bool) {
    let z = zoom();
    let next = if up {
        ZOOMS.iter().copied().find(|l| *l > z + 0.001)
    } else {
        ZOOMS.iter().rev().copied().find(|l| *l < z - 0.001)
    };
    if let Some(n) = next {
        set_zoom(n);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// A file was opened or started.
    Opened,
    /// A file's tab was closed.
    Closed,
    /// The saved state or the path changed.
    Dirty,
    /// The status readout changed (cursor, selection, page).
    Status,
}

#[derive(Default)]
struct State {
    path: Option<PathBuf>,
    /// A file is in it (until then, the tab isn't shown).
    open: bool,
    dirty: bool,
    /// Formats whose "can't keep" note was already shown for this file.
    warned: HashSet<String>,
    /// Shown in the top bar until the file is saved ("Untitled", a template's name).
    untitled: String,
    /// What saving the open file in place would lose (crate::fidelity); None while
    /// that's being worked out.
    unkept: Option<Vec<String>>,
}

/// One tab's file: its editor, its state, its autosave timer and its zoom.
struct Doc {
    tab: Tab,
    editor: Rc<dyn Editor>,
    state: State,
    autosave: Option<glib::SourceId>,
    zoom: Option<f64>,
    /// When it last came to the front, to find the front file of a kind.
    seen: u64,
}

type Listener = Rc<dyn Fn(Change)>;

thread_local! {
    /// The tabs, in the order they opened.
    static DOCS: RefCell<Vec<Doc>> = const { RefCell::new(Vec::new()) };
    static SEEN: Cell<u64> = const { Cell::new(0) };
    static LISTENERS: RefCell<Vec<(glib::WeakRef<gtk::Widget>, Listener)>> = const { RefCell::new(Vec::new()) };
    /// This run's recovery snapshot names start with this.
    static SESSION: String = format!("{}-{}", std::process::id(), recent::now());
}

fn with<R>(tab: Tab, f: impl FnOnce(&mut Doc) -> R) -> Option<R> {
    DOCS.with(|d| d.borrow_mut().iter_mut().find(|d| d.tab == tab).map(f))
}

fn state<R: Default>(tab: Tab, f: impl FnOnce(&mut State) -> R) -> R {
    with(tab, |d| f(&mut d.state)).unwrap_or_default()
}

fn current<R: Default>(f: impl FnOnce(&mut State) -> R) -> R {
    app::tab().map(|t| state(t, f)).unwrap_or_default()
}

/// The editor of the tab being worked on.
pub fn editor() -> Option<Rc<dyn Editor>> {
    app::tab().and_then(editor_in)
}

pub fn editor_in(tab: Tab) -> Option<Rc<dyn Editor>> {
    with(tab, |d| d.editor.clone())
}

/// The open tabs, in the order they opened.
pub fn tabs() -> Vec<Tab> {
    DOCS.with(|d| d.borrow().iter().filter(|d| d.state.open).map(|d| d.tab).collect())
}

/// The tab came to the front.
pub fn seen(tab: Tab) {
    let n = SEEN.with(|s| s.replace(s.get() + 1)) + 1;
    with(tab, |d| d.seen = n);
}

/// The front file of `kind`: the tab in front if it's one, or the one last in front.
pub fn front_of(kind: Kind) -> Option<Tab> {
    if let Some(f) = app::front().filter(|t| t.kind == kind && is_open_in(*t)) {
        return Some(f);
    }
    DOCS.with(|d| d.borrow().iter().filter(|d| d.tab.kind == kind && d.state.open).max_by_key(|d| d.seen).map(|d| d.tab))
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

/// The tab that has `path` open, if one has.
pub fn tab_of_path(path: &Path) -> Option<Tab> {
    DOCS.with(|d| {
        d.borrow().iter().find(|d| d.state.open && d.state.path.as_deref().is_some_and(|p| same_file(p, path))).map(|d| d.tab)
    })
}

pub fn subscribe(owner: &impl IsA<gtk::Widget>, f: impl Fn(Change) + 'static) {
    LISTENERS.with(|l| l.borrow_mut().push((owner.upcast_ref::<gtk::Widget>().downgrade(), Rc::new(f))));
}

fn emit(change: Change) {
    // Listeners may subscribe or emit in turn, so call them from a copy of the list.
    let listeners: Vec<Listener> = LISTENERS.with(|l| {
        l.borrow_mut().retain(|(w, _)| w.upgrade().is_some());
        l.borrow().iter().map(|(_, f)| f.clone()).collect()
    });
    for f in listeners {
        f(change);
    }
}

pub fn is_open() -> bool {
    current(|s| s.open)
}

pub fn is_open_in(tab: Tab) -> bool {
    state(tab, |s| s.open)
}

pub fn dirty() -> bool {
    current(|s| s.dirty)
}

pub fn dirty_in(tab: Tab) -> bool {
    state(tab, |s| s.dirty)
}

/// Some tab has unsaved changes.
pub fn any_dirty() -> bool {
    tabs().into_iter().any(dirty_in)
}

pub fn path() -> Option<PathBuf> {
    current(|s| s.path.clone())
}

pub fn path_in(tab: Tab) -> Option<PathBuf> {
    state(tab, |s| s.path.clone())
}

/// The file's name for the header, its tab and the window title.
pub fn title() -> String {
    app::tab().map(title_in).unwrap_or_default()
}

pub fn title_in(tab: Tab) -> String {
    state(tab, |s| match &s.path {
        Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        None => s.untitled.clone(),
    })
}

/// The file's name without its extension, for its tab and the header.
pub fn name_in(tab: Tab) -> String {
    state(tab, |s| match &s.path {
        Some(p) => p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        None => s.untitled.clone(),
    })
}

/// The editor calls this on every change to the content.
pub fn edited() {
    let Some(tab) = app::tab() else { return };
    let was = state(tab, |s| std::mem::replace(&mut s.dirty, true));
    if !was {
        emit(Change::Dirty);
    }
    emit(Change::Status);
    schedule_autosave();
}

/// An editor's change to its content: `tab` is the tab it was built for (None for
/// an editor outside the window, as in the checks).
pub fn edited_by(tab: Option<Tab>) {
    match tab {
        Some(t) => app::within(t, edited),
        None => edited(),
    }
}

// ---------- Saving by itself ----------

/// How long after the last change a named file saves itself.
const AUTOSAVE_AFTER: std::time::Duration = std::time::Duration::from_secs(3);

fn take_autosave() -> Option<glib::SourceId> {
    app::tab().and_then(|t| with(t, |d| d.autosave.take())).flatten()
}

fn autosaving() -> bool {
    // A file that saving would lose content from waits to be saved on purpose.
    prefs::get().autosave && path().is_some() && current(|s| s.unkept.as_ref().is_some_and(Vec::is_empty))
}

/// With autosave on, save a moment after the changes stop.
fn schedule_autosave() {
    if !autosaving() {
        return;
    }
    if let Some(id) = take_autosave() {
        id.remove();
    }
    let Some(tab) = app::tab() else { return };
    let id = glib::timeout_add_local_once(AUTOSAVE_AFTER, move || {
        app::within(tab, || {
            take_autosave();
            if dirty() && autosaving()
                && let Some(p) = path()
            {
                write_quietly(&p);
            }
        })
    });
    if with(tab, |d| d.autosave = Some(id)).is_none() {
        // The tab closed while this was being set up.
    }
}

/// The editor calls this when its status readout may have changed.
pub fn status_changed() {
    emit(Change::Status);
}

fn opened(path: Option<PathBuf>, untitled: &str, dirty: bool) {
    current(|s| {
        s.path = path;
        s.open = true;
        s.dirty = dirty;
        s.warned.clear();
        s.untitled = untitled.to_string();
        s.unkept = Some(Vec::new());
    });
    clear_recovery();
    emit(Change::Opened);
    emit(Change::Dirty);
    emit(Change::Status);
    if let Some(tab) = app::tab() {
        window::show_document(tab);
    }
    if let Some(e) = editor() {
        e.focus();
    }
}

// ---------- Tabs ----------

/// A new tab with a new editor of `kind`, not shown until a file is in it.
fn create(kind: Kind) -> Tab {
    let tab = app::new_tab(kind);
    let editor = app::within(tab, || build_editor(kind));
    DOCS.with(|d| {
        d.borrow_mut().push(Doc { tab, editor, state: State::default(), autosave: None, zoom: None, seen: 0 })
    });
    app::within(tab, || window::add_tab(tab));
    tab
}

/// Take the tab and its editor away (closed, or nothing could be put in it).
fn drop_tab(tab: Tab) {
    let gone = DOCS.with(|d| {
        let mut d = d.borrow_mut();
        let at = d.iter().position(|d| d.tab == tab)?;
        Some(d.remove(at))
    });
    let Some(mut gone) = gone else { return };
    if let Some(id) = gone.autosave.take() {
        id.remove();
    }
    window::remove_tab(tab);
    // The editor goes once the window has let go of its widgets.
    drop(gone);
}

/// "Untitled document", or "Untitled document 2" while another tab has that name.
fn unique_name(base: &str) -> String {
    let taken: HashSet<String> = DOCS.with(|d| {
        d.borrow().iter().filter(|d| d.state.open && d.state.path.is_none()).map(|d| d.state.untitled.clone()).collect()
    });
    (1..).map(|n| if n == 1 { base.to_string() } else { format!("{base} {n}") }).find(|n| !taken.contains(n)).unwrap_or_default()
}

// ---------- Unsaved changes ----------

/// Run `then` once unsaved changes are saved or let go; nothing if the user cancels.
pub fn guard(then: impl FnOnce() + 'static) {
    if !dirty() {
        then();
        return;
    }
    // Saving by itself: save now rather than ask (and ask after all if that fails).
    if autosaving()
        && let Some(p) = path()
        && write_quietly(&p)
    {
        then();
        return;
    }
    let Some(tab) = app::tab() else {
        then();
        return;
    };
    // The question is about this file: bring its tab to the front.
    window::show_document(tab);
    let name = title();
    let (dialog, card) = widgets::dialog(&format!("Save changes to “{name}”?"), 440);
    let d = widgets::label("Your changes will be lost if you don't save them.", "dim");
    d.set_wrap(true);
    card.append(&d);
    let buttons = widgets::hbox(8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(6);
    let cancel = gtk::Button::with_label("Cancel");
    let discard = gtk::Button::with_label("Don't save");
    discard.add_css_class("destructive-action");
    let save_b = gtk::Button::with_label("Save");
    save_b.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&discard);
    buttons.append(&save_b);
    card.append(&buttons);
    let then: Then = Rc::new(RefCell::new(Some(Box::new(then))));
    let dl = dialog.clone();
    cancel.connect_clicked(move |_| dl.close());
    let (dl, t) = (dialog.clone(), then.clone());
    discard.connect_clicked(move |_| {
        dl.close();
        app::within(tab, || {
            clear_recovery();
            current(|s| s.dirty = false);
            if let Some(f) = t.borrow_mut().take() {
                f();
            }
        });
    });
    let (dl, t) = (dialog.clone(), then);
    save_b.connect_clicked(move |_| {
        dl.close();
        let t = t.clone();
        app::within(tab, || {
            save_then(move || {
                if let Some(f) = t.borrow_mut().take() {
                    f();
                }
            })
        });
    });
    dialog.present();
    save_b.grab_focus();
}

/// Run `then` once every tab's unsaved changes are saved or let go (closing the window).
pub fn guard_all(then: impl FnOnce() + 'static) {
    fn next(mut tabs: Vec<Tab>, then: Box<dyn FnOnce()>) {
        match tabs.pop() {
            None => then(),
            Some(t) => app::within(t, || guard(here(move || next(tabs, then)))),
        }
    }
    let mut tabs: Vec<Tab> = tabs().into_iter().filter(|t| dirty_in(*t)).collect();
    tabs.reverse();
    // The tab in front is asked about first.
    tabs.sort_by_key(|t| Some(*t) == app::front());
    next(tabs, Box::new(then));
}

/// Close the tab, once its unsaved changes are dealt with.
pub fn close(tab: Tab) {
    app::within(tab, || {
        guard(move || {
            app::within(tab, clear_recovery);
            drop_tab(tab);
            emit(Change::Closed);
            emit(Change::Dirty);
            emit(Change::Status);
        })
    });
}

/// Run `then` if `path` is free, or once the user agrees to replace it. For
/// names we added an extension to, which the file chooser never checked.
fn confirm_replace(path: &Path, then: impl FnOnce() + 'static) {
    if !path.exists() {
        then();
        return;
    }
    let then = here(then);
    let (dialog, card) = widgets::dialog(&format!("Replace “{}”?", file_name(path)), 440);
    let d = widgets::label("A file with this name already exists. Replacing it overwrites what's in it.", "dim");
    d.set_wrap(true);
    card.append(&d);
    let buttons = widgets::hbox(8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(6);
    let cancel = gtk::Button::with_label("Cancel");
    let replace = gtk::Button::with_label("Replace");
    replace.add_css_class("destructive-action");
    buttons.append(&cancel);
    buttons.append(&replace);
    card.append(&buttons);
    let dl = dialog.clone();
    cancel.connect_clicked(move |_| dl.close());
    let dl = dialog.clone();
    let then = RefCell::new(Some(then));
    replace.connect_clicked(move |_| {
        dl.close();
        if let Some(f) = then.borrow_mut().take() {
            f();
        }
    });
    dialog.present();
    cancel.grab_focus();
}

// ---------- New and open ----------

/// Start a new file of the kind being worked on, in a new tab.
pub fn new_document(template: Option<&'static str>) {
    new_of(app::kind(), template);
}

/// Start a new file of `kind`, in a new tab. Returns the tab.
pub fn new_of(kind: Kind, template: Option<&'static str>) -> Tab {
    let tab = create(kind);
    app::within(tab, || {
        if let Some(ed) = editor() {
            ed.new_document(template);
        }
        let name = template
            .and_then(|id| templates(kind).iter().find(|t| t.id == id))
            .map(|t| t.title.to_string())
            .unwrap_or_else(|| format!("Untitled {}", kind.noun()));
        // A template starts unchanged: until it's edited there's nothing to lose.
        opened(None, &unique_name(&name), false);
    });
    tab
}

/// Open `path` in a tab of its own, or bring forward the tab that has it open.
/// Returns the tab it's in.
pub fn open(path: PathBuf) -> Option<Tab> {
    let Some(kind) = Kind::for_path(&path) else {
        window::toast(&format!("Office opens {} files.", list_exts(&all_exts())));
        return None;
    };
    if let Some(tab) = tab_of_path(&path) {
        window::show_document(tab);
        return Some(tab);
    }
    let tab = create(kind);
    let ed = editor_in(tab)?;
    let ok = app::within(tab, || match ed.open(&path) {
        Ok(()) => {
            recent::record(&path);
            opened(Some(path.clone()), "", false);
            check_fidelity(path.clone());
            true
        }
        Err(e) => {
            window::toast(&format!("Couldn't open {}: {e:#}", file_name(&path)));
            false
        }
    });
    if ok {
        Some(tab)
    } else {
        drop_tab(tab);
        None
    }
}

/// Works out in the background what saving `path` in place would lose, and says so.
fn check_fidelity(path: PathBuf) {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !["docx", "xlsx", "pptx"].contains(&ext.as_str()) {
        return;
    }
    let Some(tab) = app::tab() else { return };
    current(|s| s.unkept = None);
    glib::spawn_future_local(async move {
        let p = path.clone();
        let lost = gio::spawn_blocking(move || crate::fidelity::unkept_in(&p)).await.unwrap_or_default();
        app::within(tab, || {
            if self::path().as_deref() != Some(path.as_path()) {
                return;
            }
            current(|s| s.unkept = Some(lost.clone()));
            if !lost.is_empty() {
                window::toast(&format!(
                    "{} has {} that Office can't keep. Saving it here would remove them; Save As keeps the original.",
                    file_name(&path),
                    join_and(&lost),
                ));
            }
        });
    });
}

/// What saving to `path` would lose: something only when it's the open file and it has
/// content Office can't keep (worked out now if the check hasn't finished).
fn unkept_at(path: &Path) -> Vec<String> {
    if self::path().as_deref() != Some(path) {
        return Vec::new();
    }
    if let Some(known) = current(|s| s.unkept.clone()) {
        return known;
    }
    let lost = crate::fidelity::unkept_in(path);
    current(|s| s.unkept = Some(lost.clone()));
    lost
}

/// Save to `path`, first asking when that would lose content the file has.
fn write_checked(path: PathBuf, then: Box<dyn FnOnce()>) {
    let lost = unkept_at(&path);
    if lost.is_empty() {
        if write_to(&path) {
            then();
        }
        return;
    }
    let Some(tab) = app::tab() else { return };
    let (dialog, card) = widgets::dialog(&format!("Save over “{}”?", file_name(&path)), 460);
    let d = widgets::label(
        &format!(
            "Office can't keep this file's {}. Saving it here removes them from the file. Save a copy to keep the original as it is.",
            join_and(&lost)
        ),
        "dim",
    );
    d.set_wrap(true);
    card.append(&d);
    let buttons = widgets::hbox(8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(6);
    let cancel = gtk::Button::with_label("Cancel");
    let replace = gtk::Button::with_label("Replace anyway");
    replace.add_css_class("destructive-action");
    let copy = gtk::Button::with_label("Save a copy…");
    copy.add_css_class("suggested-action");
    buttons.append(&cancel);
    buttons.append(&replace);
    buttons.append(&copy);
    card.append(&buttons);
    let then: Then = Rc::new(RefCell::new(Some(then)));
    let dl = dialog.clone();
    cancel.connect_clicked(move |_| dl.close());
    let (dl, t, p) = (dialog.clone(), then.clone(), path);
    replace.connect_clicked(move |_| {
        dl.close();
        app::within(tab, || {
            current(|s| s.unkept = Some(Vec::new()));
            if write_to(&p)
                && let Some(f) = t.borrow_mut().take()
            {
                f();
            }
        });
    });
    let dl = dialog.clone();
    copy.connect_clicked(move |_| {
        dl.close();
        let t = then.clone();
        app::within(tab, || {
            save_as_then(move || {
                if let Some(f) = t.borrow_mut().take() {
                    f();
                }
            })
        });
    });
    dialog.present();
    copy.grab_focus();
}

fn list_exts(exts: &[&str]) -> String {
    let dotted: Vec<String> = exts.iter().map(|e| format!(".{e}")).collect();
    match dotted.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => dotted.join(""),
    }
}

fn all_exts() -> Vec<&'static str> {
    Kind::ALL.iter().flat_map(|k| k.opens().iter().copied()).collect()
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn start_folder() -> gio::File {
    let dir = path().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(paths::documents_dir);
    gio::File::for_path(if dir.is_dir() { dir } else { paths::home() })
}

fn filter(name: &str, exts: &[&str]) -> gtk::FileFilter {
    let f = gtk::FileFilter::new();
    f.set_name(Some(name));
    for e in exts {
        f.add_suffix(e);
    }
    f
}

/// Open any files Office opens (several at once if picked); each gets a tab.
pub fn open_dialog() {
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    let all = filter("Office files", &all_exts());
    filters.append(&all);
    for kind in Kind::ALL {
        filters.append(&filter(kind.plural(), kind.opens()));
    }
    let dialog = gtk::FileDialog::builder()
        .title("Open files")
        .modal(true)
        .filters(&filters)
        .default_filter(&all)
        .build();
    dialog.set_initial_folder(Some(&start_folder()));
    dialog.open_multiple(window::window().as_ref(), gio::Cancellable::NONE, |res| {
        let Ok(files) = res else { return };
        for file in files.iter::<gio::File>().flatten() {
            if let Some(p) = file.path() {
                open(p);
            }
        }
    });
}

// ---------- Saving ----------

pub fn save() {
    save_then(|| {});
}

/// Save to the current file (asking for a name the first time), then run `then`.
fn save_then(then: impl FnOnce() + 'static) {
    match path() {
        Some(p) => write_checked(p, Box::new(then)),
        None => save_as_then(then),
    }
}

pub fn save_as() {
    save_as_then(|| {});
}

fn save_as_then(then: impl FnOnce() + 'static) {
    if !is_open() {
        return;
    }
    let Some(tab) = app::tab() else { return };
    let kind = tab.kind;
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    for (name, ext) in kind.save_formats() {
        filters.append(&filter(name, &[ext]));
    }
    let stem = match path() {
        Some(p) => p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        None => title(),
    };
    let ext = path()
        .and_then(|p| p.extension().map(|e| e.to_string_lossy().to_lowercase()))
        .unwrap_or_else(|| kind.native_ext().into());
    let dialog = gtk::FileDialog::builder()
        .title(format!("Save the {}", kind.noun()))
        .modal(true)
        .filters(&filters)
        .initial_name(format!("{stem}.{ext}"))
        .build();
    dialog.set_initial_folder(Some(&start_folder()));
    dialog.save(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
        let Ok(file) = res else { return };
        let Some(mut p) = file.path() else { return };
        app::within(tab, move || {
            if !is_open() {
                return;
            }
            let known = p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .is_some_and(|e| kind.save_formats().iter().any(|(_, x)| *x == e));
            if known {
                write_checked(p, Box::new(then));
                return;
            }
            let name = format!("{}.{}", file_name(&p), kind.native_ext());
            p.set_file_name(name);
            let target = p.clone();
            confirm_replace(&target, move || {
                if write_to(&p) {
                    then();
                }
            });
        });
    });
}

/// Save to `path` now, without a dialog (the control interface). True when it worked.
pub fn save_to(path: &Path) -> bool {
    let lost = unkept_at(path);
    if !lost.is_empty() {
        window::toast(&format!(
            "Not saved: Office can't keep {}'s {}. Save it as a copy instead.",
            file_name(path),
            join_and(&lost)
        ));
        return false;
    }
    write_to(path)
}

/// Save to `path` now. True when it worked.
fn write_to(path: &Path) -> bool {
    save_file(path, false)
}

/// Save without a "Saved" note (still telling of failures, and once of what a
/// format can't keep).
fn write_quietly(path: &Path) -> bool {
    save_file(path, true)
}

fn save_file(path: &Path, quiet: bool) -> bool {
    if let Some(id) = take_autosave() {
        id.remove();
    }
    let Some(ed) = editor() else { return false };
    // Another tab has this file open: saving over it would leave two tabs on one file.
    if let Some(other) = tab_of_path(path).filter(|t| Some(*t) != app::tab()) {
        window::toast(&format!("{} is open in another tab. Close that tab first, or save under another name.", title_in(other)));
        return false;
    }
    match ed.save(path) {
        Ok(lost) => {
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let first = current(|s| {
                let changed_path = s.path.as_deref() != Some(path);
                s.path = Some(path.to_path_buf());
                s.dirty = false;
                if changed_path {
                    s.warned.clear();
                    // A file this app wrote: nothing in it that it can't keep.
                    s.unkept = Some(Vec::new());
                }
                s.warned.insert(ext.clone())
            });
            recent::record(path);
            clear_recovery();
            emit(Change::Dirty);
            emit(Change::Status);
            if first && !lost.is_empty() {
                window::toast(&format!(
                    "Saved {}. A .{ext} file can't keep {}; save as .{} to keep everything.",
                    file_name(path),
                    join_and(&lost),
                    app::kind().native_ext()
                ));
            } else if !quiet {
                window::toast(&format!("Saved {}.", file_name(path)));
            }
            true
        }
        Err(e) => {
            window::toast(&format!("Couldn't save {}: {e:#}", file_name(path)));
            false
        }
    }
}

pub fn join_and(items: &[String]) -> String {
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => items.join(""),
    }
}

// ---------- PDF and printing ----------

pub fn export_pdf_dialog() {
    let Some(ed) = editor() else { return };
    if !is_open() {
        return;
    }
    export_pages_dialog("Export as PDF", "", move || ed.pages());
}

/// The file's name without its extension, or the document's title.
pub fn stem() -> String {
    path().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned())).unwrap_or_else(title)
}

/// Ask where to save a PDF of the pages `make` gives (`suffix` goes after the
/// file's name: " handouts"), then write it.
pub fn export_pages_dialog(title: &str, suffix: &str, make: impl Fn() -> Rc<dyn Pages> + 'static) {
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter("PDF", &["pdf"]));
    let stem = stem();
    let dialog = gtk::FileDialog::builder()
        .title(title)
        .modal(true)
        .filters(&filters)
        .initial_name(format!("{stem}{suffix}.pdf"))
        .build();
    dialog.set_initial_folder(Some(&start_folder()));
    let tab = app::tab();
    dialog.save(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
        let Ok(file) = res else { return };
        let Some(mut p) = file.path() else { return };
        let export = move |p: PathBuf| match print::export_pdf(make().as_ref(), &p) {
            Ok(()) => window::toast(&format!("Exported {}.", file_name(&p))),
            Err(e) => window::toast(&format!("Couldn't export the PDF: {e:#}")),
        };
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            export(p);
            return;
        }
        let name = format!("{}.pdf", file_name(&p));
        p.set_file_name(name);
        let target = p.clone();
        let ask = move || confirm_replace(&target, move || export(p));
        match tab {
            Some(t) => app::within(t, ask),
            None => ask(),
        }
    });
}

pub fn print_now() {
    if let Some(ed) = editor()
        && is_open()
    {
        print::print(ed.pages(), &title());
    }
}

// ---------- Crash recovery ----------

fn session() -> String {
    SESSION.with(|s| s.clone())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RecoveryInfo {
    /// The file the work belongs to, if it had been saved before.
    path: Option<PathBuf>,
    title: String,
    saved: u64,
}

pub struct Recovery {
    pub kind: Kind,
    pub snapshot: PathBuf,
    pub path: Option<PathBuf>,
    pub title: String,
    pub saved: u64,
}

/// The tab's snapshot: `<session>-<tab>.<ext>` in its kind's recovery folder.
fn snapshot_file(tab: Tab) -> PathBuf {
    paths::recovery_dir(tab.kind.id()).join(format!("{}-{}.{}", session(), tab.id, tab.kind.native_ext()))
}

/// True for a snapshot this run wrote (`<session>-<tab>`), which isn't left over.
fn is_mine(stem: &str, session: &str) -> bool {
    stem.strip_prefix(session).and_then(|rest| rest.strip_prefix('-')).is_some_and(|id| id.parse::<u32>().is_ok())
}

/// Every few seconds, keep a copy of each tab's unsaved work.
pub fn start_recovery_timer() {
    glib::timeout_add_seconds_local(10, || {
        for tab in tabs() {
            app::within(tab, || {
                if dirty() && prefs::get().recovery {
                    write_recovery();
                }
            });
        }
        glib::ControlFlow::Continue
    });
}

fn write_recovery() {
    let Some(tab) = app::tab() else { return };
    let Some(ed) = editor() else { return };
    let Ok(bytes) = ed.snapshot() else { return };
    let file = snapshot_file(tab);
    let info = RecoveryInfo { path: path(), title: title(), saved: recent::now() };
    if cmd::atomic_write(&file, bytes).is_ok()
        && let Ok(text) = serde_json::to_string(&info)
    {
        let _ = cmd::atomic_write(&file.with_extension("json"), text);
    }
}

pub fn clear_recovery() {
    let Some(tab) = app::tab() else { return };
    let file = snapshot_file(tab);
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_file(file.with_extension("json"));
}

/// Every tab's recovery copy from this run (closing the window).
pub fn clear_all_recovery() {
    for tab in tabs() {
        app::within(tab, clear_recovery);
    }
}

/// Work left behind by an earlier run that ended without saving, newest first.
pub fn leftovers() -> Vec<Recovery> {
    let mine = session();
    let mut out: Vec<Recovery> = Vec::new();
    for kind in Kind::ALL {
        let Ok(rd) = std::fs::read_dir(paths::recovery_dir(kind.id())) else { continue };
        out.extend(
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .filter(|p| p.file_stem().is_some_and(|s| !is_mine(&s.to_string_lossy(), &mine)))
                .filter_map(|p| {
                    let info: RecoveryInfo = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
                    let snapshot = p.with_extension(kind.native_ext());
                    snapshot.exists().then_some(Recovery { kind, snapshot, path: info.path, title: info.title, saved: info.saved })
                }),
        );
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.saved));
    out
}

pub fn discard(r: &Recovery) {
    let _ = std::fs::remove_file(&r.snapshot);
    let _ = std::fs::remove_file(r.snapshot.with_extension("json"));
}

/// Bring back left-over work: into the tab that has its file open (once that tab's
/// own changes are dealt with), or a new tab.
pub fn restore(r: Recovery) {
    let (kind, open_in) = (r.kind, r.path.as_deref().and_then(tab_of_path));
    let load = move || {
        let Some(ed) = editor() else { return false };
        let loaded = std::fs::read(&r.snapshot).map_err(anyhow::Error::from).and_then(|b| ed.restore(&b));
        match loaded {
            Ok(()) => {
                discard(&r);
                let title = if r.path.is_some() { r.title.clone() } else { unique_name(&r.title) };
                opened(r.path.clone(), &title, true);
                window::toast("Restored your unsaved work. Save it to keep it.");
                if let Some(p) = r.path.clone() {
                    check_fidelity(p);
                }
                true
            }
            Err(e) => {
                window::toast(&format!("Couldn't restore that work: {e:#}"));
                false
            }
        }
    };
    if let Some(tab) = open_in {
        app::within(tab, || {
            guard(here(move || {
                load();
            }))
        });
        return;
    }
    let tab = create(kind);
    if !app::within(tab, load) {
        drop_tab(tab);
    }
}

// ---------- The ⋯ menu ----------

fn menu_row(label: &str, keys: &str, act: Action, popover: &gtk::Popover) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("menu-item");
    let content = widgets::hbox(16);
    let l = widgets::label(label, "");
    l.set_hexpand(true);
    content.append(&l);
    if !keys.is_empty() {
        content.append(&widgets::label(keys, "dim"));
    }
    b.set_child(Some(&content));
    let p = popover.clone();
    b.connect_clicked(move |_| {
        p.popdown();
        act();
    });
    b
}

/// The file menu at the end of every editor's toolbar: the file commands, then
/// the kind's own (`extra`). Built with the editor, so it works on that editor's tab.
pub fn file_menu(extra: Vec<MenuItem>) -> gtk::MenuButton {
    let kind = app::kind();
    let tab = app::tab();
    let on_tab = move |f: Action| -> Action {
        Rc::new(move || match tab {
            Some(t) => app::within(t, || f()),
            None => f(),
        })
    };
    let menu = gtk::MenuButton::new();
    menu.set_label("File");
    menu.add_css_class("commandbar-file");
    menu.set_tooltip_text(Some("File: save, export, print"));
    menu.set_valign(gtk::Align::Center);
    let popover = gtk::Popover::new();
    popover.add_css_class("menu-popover");
    let list = widgets::vbox(0);
    let rows: Vec<(String, &str, Action)> = vec![
        (format!("New blank {}", kind.noun()), "Ctrl+N", Rc::new(move || {
            new_of(kind, None);
        })),
        ("Open…".into(), "Ctrl+O", Rc::new(open_dialog)),
        ("Save".into(), "Ctrl+S", Rc::new(save)),
        ("Save as…".into(), "Ctrl+Shift+S", Rc::new(save_as)),
        ("Export as PDF…".into(), "Ctrl+E", Rc::new(export_pdf_dialog)),
        ("Print…".into(), "Ctrl+P", Rc::new(print_now)),
    ];
    for (label, keys, act) in rows {
        list.append(&menu_row(&label, keys, on_tab(act), &popover));
    }
    if !extra.is_empty() {
        list.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        for (label, act) in extra {
            list.append(&menu_row(label, "", on_tab(act), &popover));
        }
    }
    popover.set_child(Some(&list));
    menu.set_popover(Some(&popover));
    menu
}

/// Several files of each kind open at once, each in its own tab. Needs GTK, so it
/// runs from `gtk_tests::editors_behave`. Nothing here saves or records a file.
#[cfg(test)]
pub fn tabs_behave() {
    let letter = new_of(Kind::Write, Some("letter"));
    let blank = new_of(Kind::Write, None);
    let blank2 = new_of(Kind::Write, None);
    let budget = new_of(Kind::Sheets, Some("monthly-budget"));
    let budget2 = new_of(Kind::Sheets, Some("monthly-budget"));
    let talk = new_of(Kind::Present, None);
    assert_eq!(tabs(), vec![letter, blank, blank2, budget, budget2, talk]);
    assert_eq!(name_in(letter), "Personal letter");
    assert_eq!(name_in(blank), "Untitled document");
    assert_eq!(name_in(blank2), "Untitled document 2");
    assert_eq!(name_in(talk), "Untitled presentation");
    let editors: Vec<Rc<dyn Editor>> = tabs().into_iter().filter_map(editor_in).collect();
    assert_eq!(editors.len(), 6);
    for (i, a) in editors.iter().enumerate() {
        for b in &editors[i + 1..] {
            assert!(!Rc::ptr_eq(a, b), "each tab has its own editor");
        }
    }
    assert!(tabs().into_iter().all(|t| !dirty_in(t)), "new files start unchanged");

    // An edit made in a tab that isn't in front marks that tab, not the front one.
    app::set_front(Some(blank));
    let n = editor_in(letter).and_then(|e| e.find()).map_or(0, |f| f.replace_all("Dear", "Hello", true));
    assert!(n > 0);
    assert!(dirty_in(letter));
    assert!(!dirty_in(blank) && !dirty_in(blank2));
    app::set_front(Some(budget));
    let n = editor_in(budget2).and_then(|e| e.find()).map_or(0, |f| f.replace_all("Groceries", "Food", true));
    assert!(n > 0);
    assert!(dirty_in(budget2) && !dirty_in(budget));
    // The other workbook still has its own text.
    let left = editor_in(budget).and_then(|e| e.find()).map_or(0, |f| f.count("Groceries", true));
    assert!(left > 0);
    assert_eq!(editor_in(budget2).and_then(|e| e.find()).map_or(1, |f| f.count("Groceries", true)), 0);

    // A file that's open comes forward instead of opening a second time.
    let p = std::env::temp_dir().join(format!("nyx-office-tabs-{}.nwrite", std::process::id()));
    state(letter, |s| s.path = Some(p.clone()));
    assert_eq!(tab_of_path(&p), Some(letter));
    assert_eq!(open(p.clone()), Some(letter));
    assert_eq!(tabs().len(), 6);
    // Saving another tab over that file is refused, and writes nothing.
    assert!(!app::within(blank, || save_file(&p, true)));
    assert!(!p.exists());

    // The front file of a kind: the one in front, or the one last in front.
    app::set_front(Some(budget2));
    seen(budget2);
    assert_eq!(front_of(Kind::Sheets), Some(budget2));
    app::set_front(Some(blank));
    seen(blank);
    assert_eq!(front_of(Kind::Sheets), Some(budget2));
    assert_eq!(front_of(Kind::Write), Some(blank));
    assert_eq!(front_of(Kind::Present), Some(talk));

    // Nyx Assistant's tools work on the front document, and nowhere else.
    app::set_front(Some(budget));
    let insert = serde_json::json!({"text": "Minutes\nTea at four", "style": "heading1", "at": "end"});
    assert!(crate::write::control::handle("write.insert_text", insert).is_ok());
    assert!(dirty_in(blank) && !dirty_in(blank2));
    let read = crate::write::control::handle("write.read", serde_json::json!({})).unwrap_or_default();
    let text = read["text"].as_str().unwrap_or_default();
    assert!(text.contains("# Minutes") && text.contains("# Tea at four"), "{text}");
    assert_eq!(read["document"]["title"], "Untitled document");
    let other = app::within(letter, || crate::write::control_text().unwrap_or_default());
    assert!(!other.contains("Tea at four"));
    app::set_front(Some(blank));

    // Each tab keeps its own zoom.
    with(blank, |d| d.zoom = Some(1.5));
    with(blank2, |d| d.zoom = Some(0.75));
    assert_eq!((zoom_of(Some(blank)), zoom_of(Some(blank2))), (1.5, 0.75));

    // Closing a tab leaves the others as they were; its name is free again.
    close(blank2);
    assert!(!tabs().contains(&blank2));
    assert!(editor_in(blank2).is_none());
    assert!(dirty_in(letter) && dirty_in(budget2));
    assert_eq!(path_in(letter), Some(p.clone()));
    assert_eq!(editor_in(budget).and_then(|e| e.find()).map_or(0, |f| f.count("Groceries", true)), left);
    let again = new_of(Kind::Write, None);
    assert_eq!(name_in(again), "Untitled document 2");

    app::set_front(None);
    for t in tabs() {
        drop_tab(t);
    }
    assert!(tabs().is_empty());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_of_this_run_are_not_left_over() {
        assert!(is_mine("123-456-7", "123-456"));
        assert!(!is_mine("123-456", "123-456"));
        assert!(!is_mine("123-4567-2", "123-456"));
        assert!(!is_mine("999-456-7", "123-456"));
        assert!(!is_mine("123-456-x", "123-456"));
    }

    #[test]
    fn names_join_with_and() {
        assert_eq!(join_and(&["charts".into()]), "charts");
        assert_eq!(join_and(&["a".into(), "b".into(), "c".into()]), "a, b and c");
    }
}
