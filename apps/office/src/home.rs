//! Home and the pages beside it. Home: work left unsaved by a crash to bring back,
//! a new document, spreadsheet or presentation (blank or from a template), and the
//! recent files of every kind with a filter for the kind. Recent: every recent file.
//! Templates: every kind's templates together.

use crate::app::{self, Kind};
use crate::widgets;
use crate::{doc, paths, recent, window};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::Path;

/// Recent files on Home; Recent has them all.
const HOME_ROWS: usize = 7;
const HOME_ROWS_NARROW: usize = 6;

struct Home {
    banners: gtk::Box,
    /// Home's recent files, then the Recent page's.
    lists: Vec<(gtk::Box, bool)>,
    /// The kind filters (All, Documents, Sheets, Slides) on Home and on Recent.
    filters: Vec<gtk::Box>,
    /// What the New cards hide in a narrow window.
    wide_only: Vec<gtk::Widget>,
    query: String,
    filter: Option<Kind>,
    narrow: bool,
}

thread_local! {
    static HOME: RefCell<Home> = RefCell::new(Home {
        banners: widgets::vbox(10),
        lists: Vec::new(),
        filters: Vec::new(),
        wide_only: Vec::new(),
        query: String::new(),
        filter: None,
        narrow: false,
    });
    /// Set while the filters are being set to match: their handlers ignore it.
    static SYNCING: Cell<bool> = const { Cell::new(false) };
}

/// The page called `name` (`home`, `recent` or `templates`).
pub fn build(name: &str) -> gtk::Widget {
    let page = widgets::page(name);
    page.body.add_css_class("office-home");
    let content = widgets::vbox(0);
    match name {
        "home" => build_home(&content),
        "recent" => build_recent(&content),
        _ => build_templates(&content),
    }
    page.body.append(&nyx_ui::clamp::Clamp::new(&content, 980));
    if name == "home" {
        refresh();
        // Saving, opening and closing change the lists.
        doc::subscribe(&page.body, |c| {
            if matches!(c, doc::Change::Opened | doc::Change::Dirty | doc::Change::Closed) {
                refresh();
            }
        });
    }
    page.root.upcast()
}

fn heading(text: &str) -> gtk::Box {
    let row = widgets::hbox(8);
    row.add_css_class("home-head");
    let l = widgets::label(text, "home-title");
    l.set_hexpand(true);
    row.append(&l);
    row
}

fn build_home(content: &gtk::Box) {
    let banners = HOME.with(|h| h.borrow().banners.clone());
    banners.add_css_class("home-banners");
    content.append(&banners);

    let head = heading("New");
    head.add_css_class("first");
    content.append(&head);
    let cards = widgets::hbox(14);
    cards.set_homogeneous(true);
    cards.add_css_class("new-cards");
    for kind in Kind::ALL {
        cards.append(&new_card(kind));
    }
    content.append(&cards);

    let head = heading("Recent");
    head.append(&kind_filter());
    content.append(&head);
    content.append(&recent_list(true));
    content.append(&open_line());
}

fn build_recent(content: &gtk::Box) {
    let head = heading("Recent files");
    head.add_css_class("first");
    head.append(&kind_filter());
    content.append(&head);
    content.append(&recent_list(false));
    content.append(&open_line());
}

fn build_templates(content: &gtk::Box) {
    for (i, kind) in Kind::ALL.into_iter().enumerate() {
        let head = heading(kind.plural());
        if i == 0 {
            head.add_css_class("first");
        }
        head.prepend(&window::kind_icon(kind));
        content.append(&head);
        let flow = gtk::FlowBox::new();
        flow.add_css_class("template-grid");
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_column_spacing(10);
        flow.set_row_spacing(10);
        flow.set_homogeneous(true);
        flow.set_min_children_per_line(1);
        flow.set_max_children_per_line(4);
        flow.set_valign(gtk::Align::Start);
        let blank = template_card(kind, kind.icon(), &format!("Blank {}", kind.noun()), "Start from an empty page.");
        blank.connect_clicked(move |_| start(kind, None));
        flow.append(&blank);
        for t in doc::templates(kind) {
            let c = template_card(kind, t.icon, t.title, t.summary);
            let id = t.id;
            c.connect_clicked(move |_| start(kind, Some(id)));
            flow.append(&c);
        }
        content.append(&flow);
    }
}

/// Start a file, only from these pages: a second click as the page fades out isn't
/// another new file.
fn start(kind: Kind, template: Option<&'static str>) {
    if app::front().is_none() {
        doc::new_of(kind, template);
    }
}

fn template_card(kind: Kind, icon: &str, title: &str, summary: &str) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("template-card");
    let content = widgets::hbox(12);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(20);
    image.set_valign(gtk::Align::Start);
    image.add_css_class(&kind.color_class());
    content.append(&image);
    let text = widgets::vbox(2);
    let t = widgets::label(title, "template-title");
    t.set_wrap(true);
    t.set_max_width_chars(24);
    text.append(&t);
    let s = widgets::label(summary, "template-summary");
    s.set_wrap(true);
    // Narrow labels let several cards share a line.
    s.set_max_width_chars(24);
    s.set_width_chars(18);
    text.append(&s);
    content.append(&text);
    b.set_child(Some(&content));
    b.set_tooltip_text(Some(summary));
    b
}

/// A New card: the kind, then Blank and its templates as chips.
fn new_card(kind: Kind) -> gtk::Box {
    let card = widgets::vbox(12);
    card.add_css_class("new-card");
    let head = widgets::hbox(12);
    let tile = gtk::CenterBox::new();
    tile.add_css_class("kind-tile");
    tile.add_css_class(&kind.color_class());
    tile.set_valign(gtk::Align::Center);
    let image = gtk::Image::from_icon_name(kind.icon());
    image.set_pixel_size(20);
    tile.set_center_widget(Some(&image));
    head.append(&tile);
    let text = widgets::vbox(1);
    text.set_valign(gtk::Align::Center);
    text.append(&widgets::label(kind.title(), "new-card-title"));
    let sub = widgets::label("Blank, or from a template", "new-card-sub");
    text.append(&sub);
    head.append(&text);
    card.append(&head);
    HOME.with(|h| h.borrow_mut().wide_only.extend([tile.upcast(), sub.upcast()]));

    let chips = widgets::wrap_box(6, 6);
    chips.add_css_class("chips");
    chips.set_valign(gtk::Align::Start);
    let blank = gtk::Button::new();
    blank.add_css_class("chip");
    blank.add_css_class("chip-blank");
    blank.add_css_class(&kind.color_class());
    let face = widgets::hbox(4);
    face.append(&gtk::Image::from_icon_name("nxo-plus-symbolic"));
    face.append(&gtk::Label::new(Some("Blank")));
    blank.set_child(Some(&face));
    blank.set_tooltip_text(Some(&format!("A blank {}", kind.noun())));
    blank.connect_clicked(move |_| start(kind, None));
    chips.append(&blank);
    for t in doc::templates(kind) {
        let chip = gtk::Button::with_label(t.short);
        chip.add_css_class("chip");
        chip.add_css_class(&kind.color_class());
        chip.set_tooltip_text(Some(&format!("{}: {}", t.title, t.summary)));
        let id = t.id;
        chip.connect_clicked(move |_| start(kind, Some(id)));
        chips.append(&chip);
    }
    card.append(&chips);
    card
}

/// All, Documents, Sheets, Slides.
fn kind_filter() -> gtk::Box {
    let mut options = vec![("all".to_string(), "All".to_string())];
    options.extend(Kind::ALL.map(|k| (k.id().to_string(), k.filter_label().to_string())));
    let seg = widgets::segmented(&options, "all", |id| {
        if !SYNCING.with(|s| s.get()) {
            set_filter(Kind::parse(&id));
            window::navigate(&window::current());
        }
    });
    seg.add_css_class("kind-filter");
    HOME.with(|h| h.borrow_mut().filters.push(seg.clone()));
    seg
}

fn recent_list(capped: bool) -> gtk::Box {
    let list = widgets::vbox(0);
    list.add_css_class("recent-table");
    HOME.with(|h| h.borrow_mut().lists.push((list.clone(), capped)));
    list
}

fn open_line() -> gtk::Box {
    let row = widgets::hbox(10);
    row.add_css_class("open-line");
    let open = gtk::Button::new();
    open.add_css_class("open-file");
    let face = widgets::hbox(6);
    face.append(&gtk::Image::from_icon_name("nxo-folder-open-symbolic"));
    face.append(&gtk::Label::new(Some("Open a file…")));
    open.set_child(Some(&face));
    open.set_tooltip_text(Some("Open a file (Ctrl+O)"));
    open.connect_clicked(|_| doc::open_dialog());
    row.append(&open);
    let note = widgets::label("Word, Excel, PowerPoint, OpenDocument, Markdown, CSV and text files.", "open-note");
    note.set_wrap(true);
    note.set_hexpand(true);
    row.append(&note);
    row
}

/// The header's search filters the recent files.
pub fn set_query(q: &str) {
    HOME.with(|h| h.borrow_mut().query = q.to_lowercase());
    refresh();
}

/// Show only one kind's recent files (None: every kind).
pub fn set_filter(kind: Option<Kind>) {
    HOME.with(|h| h.borrow_mut().filter = kind);
    refresh();
}

pub fn filter() -> Option<Kind> {
    HOME.with(|h| h.borrow().filter)
}

pub fn set_narrow(narrow: bool) {
    HOME.with(|h| h.borrow_mut().narrow = narrow);
    let wide_only = HOME.with(|h| h.borrow().wide_only.clone());
    for w in wide_only {
        w.set_visible(!narrow);
    }
    refresh();
}

pub fn refresh() {
    let (banners, lists, filters, query, filter, narrow) = HOME.with(|h| {
        let h = h.borrow();
        (h.banners.clone(), h.lists.clone(), h.filters.clone(), h.query.clone(), h.filter, h.narrow)
    });

    while let Some(c) = banners.first_child() {
        banners.remove(&c);
    }
    for r in doc::leftovers().into_iter().take(3) {
        banners.append(&recovery_banner(r));
    }
    banners.set_visible(banners.first_child().is_some());

    SYNCING.with(|s| s.set(true));
    let id = filter.map_or("all", |k| k.id());
    for seg in &filters {
        let mut child = seg.first_child();
        let mut i = 0;
        while let Some(c) = child {
            if let Some(b) = c.downcast_ref::<gtk::ToggleButton>() {
                let mine = if i == 0 { "all" } else { Kind::ALL[i - 1].id() };
                b.set_active(mine == id);
            }
            i += 1;
            child = c.next_sibling();
        }
    }
    SYNCING.with(|s| s.set(false));

    let items: Vec<(Kind, recent::Recent)> = recent::all()
        .into_iter()
        .filter(|(k, _)| filter.is_none_or(|f| f == *k))
        .filter(|(_, r)| query.is_empty() || r.path.to_string_lossy().to_lowercase().contains(&query))
        .collect();
    for (list, capped) in lists {
        while let Some(c) = list.first_child() {
            list.remove(&c);
        }
        if items.is_empty() {
            let text = if !query.is_empty() {
                format!("No recent files match “{query}”.")
            } else if let Some(k) = filter {
                format!("{} you open or save show up here.", k.plural())
            } else {
                "Files you open or save show up here.".to_string()
            };
            let empty = widgets::label(&text, "recent-empty");
            list.append(&empty);
            continue;
        }
        let cap = if !capped || !query.is_empty() {
            usize::MAX
        } else if narrow {
            HOME_ROWS_NARROW
        } else {
            HOME_ROWS
        };
        for (kind, r) in items.iter().take(cap) {
            list.append(&recent_row(*kind, r, narrow));
        }
    }
}

/// Where a file is, for its row: "Documents › Garden club", or the folder's path.
fn place(path: &Path) -> String {
    let Some(dir) = path.parent() else { return String::new() };
    match dir.strip_prefix(paths::home()) {
        Ok(rel) if rel.as_os_str().is_empty() => "Home folder".into(),
        Ok(rel) => rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join(" › "),
        Err(_) => paths::pretty(dir),
    }
}

fn recovery_banner(r: doc::Recovery) -> gtk::Box {
    let b = widgets::hbox(14);
    b.add_css_class("recover-banner");
    let icon = gtk::Image::from_icon_name("nxo-rotate-ccw-symbolic");
    icon.add_css_class("recover-icon");
    b.append(&icon);
    let text = widgets::vbox(2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    text.append(&widgets::label("Bring back your unsaved work?", "recover-title"));
    let detail = widgets::label(&format!("“{}” closed unexpectedly {}.", r.title, recent::when_at(r.saved)), "recover-detail");
    detail.set_wrap(true);
    text.append(&detail);
    b.append(&text);
    let discard = gtk::Button::with_label("Discard");
    discard.add_css_class("flat");
    discard.set_valign(gtk::Align::Center);
    let restore = gtk::Button::with_label("Bring it back");
    restore.add_css_class("suggested-action");
    restore.set_valign(gtk::Align::Center);
    let snapshot = r.snapshot.clone();
    discard.connect_clicked(move |_| {
        if let Some(r) = doc::leftovers().into_iter().find(|r| r.snapshot == snapshot) {
            doc::discard(&r);
        }
        refresh();
    });
    let cell = std::rc::Rc::new(RefCell::new(Some(r)));
    restore.connect_clicked(move |_| {
        if let Some(r) = cell.borrow_mut().take() {
            doc::restore(r);
        }
    });
    b.append(&discard);
    b.append(&restore);
    b
}

fn recent_row(kind: Kind, r: &recent::Recent, narrow: bool) -> gtk::Box {
    let item = widgets::hbox(0);
    item.add_css_class("recent-item");
    let main = gtk::Button::new();
    main.add_css_class("recent-main");
    main.set_hexpand(true);
    let content = widgets::hbox(12);
    content.append(&window::kind_icon(kind));
    let stem = r.path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = widgets::label(&stem, "recent-name");
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&name);
    if !narrow {
        let place = widgets::label(&place(&r.path), "recent-where");
        place.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        place.set_width_chars(26);
        place.set_max_width_chars(26);
        content.append(&place);
    }
    let ext = r.path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
    let fmt = gtk::Label::new(Some(&ext));
    fmt.add_css_class("format-badge");
    fmt.set_width_chars(7);
    fmt.set_valign(gtk::Align::Center);
    content.append(&fmt);
    let when = widgets::label(&recent::when(r.opened), "recent-when");
    when.set_xalign(1.0);
    when.set_width_chars(13);
    content.append(&when);
    main.set_child(Some(&content));
    main.set_tooltip_text(Some(&paths::pretty(&r.path)));
    main.update_property(&[gtk::accessible::Property::Label(&stem)]);
    let p = r.path.clone();
    main.connect_clicked(move |_| {
        doc::open(p.clone());
    });
    item.append(&main);

    let forget = widgets::bar_button("nxo-x-symbolic", "Remove from this list");
    forget.add_css_class("recent-forget");
    forget.update_property(&[gtk::accessible::Property::Label(&format!("Remove {stem} from this list"))]);
    let p = r.path.clone();
    forget.connect_clicked(move |_| {
        recent::forget(&p);
        refresh();
        window::navigate(&window::current());
    });
    item.append(&forget);

    let (open_path, forget_path) = (r.path.clone(), r.path.clone());
    nyx_ui::menu::on_right_click(&item, move |w, x, y| {
        let (o, f) = (open_path.clone(), forget_path.clone());
        nyx_ui::menu::popup_at(
            w,
            x,
            y,
            vec![
                nyx_ui::menu::item("Open", move || {
                    doc::open(o.clone());
                })
                .icon("nxo-folder-open-symbolic"),
                nyx_ui::menu::item("Remove from this list", move || {
                    recent::forget(&f);
                    refresh();
                    window::navigate(&window::current());
                })
                .icon("nxo-x-symbolic"),
            ],
        );
    });
    item
}
