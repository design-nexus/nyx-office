//! The Office window (Nyx Design 2.0): one window for documents, spreadsheets and
//! presentations, like a browser with tabs. The header has the app, the file in front
//! and its tools (or, on Home, search, Open and New); the sidebar has Home, Recent,
//! the files by kind and the templates, then the front file's own group (the outline
//! or the slides); the page has a tab per open file (any number of each kind, each
//! in its kind's colour) over Home, Recent, Templates or the editor of the tab in
//! front. Each tab has its own page, header tools, Inspector, sidebar group and find
//! bar, made when it opens (`add_tab`) and taken away when it closes. Settings open
//! as a card over the window.

use crate::app::{self, Kind, Tab};
use crate::{doc, home, prefs, settings_dialog, theme, widgets};
use gtk::prelude::*;
use nyx_ui::chrome;
use nyx_ui::commandbar::CommandBar;
use nyx_ui::inspector::Inspector;
use nyx_ui::sidebar::{self, NavItem};
use gtk::{gdk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The pages that aren't a file: Home, every recent file, and the templates.
const PAGES: [&str; 3] = ["home", "recent", "templates"];

struct Nav {
    home: NavItem,
    recent: NavItem,
    kinds: [NavItem; 3],
    templates: NavItem,
    /// Where each tab's own group goes.
    list: gtk::Box,
    /// Each editor's own group (a document's outline, a presentation's slides), shown with its tab.
    groups: Vec<(Tab, gtk::Box)>,
    foot: gtk::Box,
    foot_label: gtk::Label,
}

struct Ui {
    window: gtk::ApplicationWindow,
    stack: gtk::Stack,
    nav: gtk::Box,
    items: Nav,
    /// The page showing: one of `PAGES`, or a tab's (`Tab::page`).
    current: String,
    overlay: gtk::Overlay,
    /// Hidden in fullscreen: the header and the tabs.
    chrome: Vec<gtk::Widget>,
    tabs: gtk::Box,
    tabs_bar: gtk::Box,
    /// The open files' tabs, in the order they opened.
    order: Vec<Tab>,
    /// The front file's name and its unsaved dot, after the sidebar toggle.
    doc_title: gtk::Box,
    doc_name: gtk::Label,
    doc_dot: gtk::Box,
    /// Open… and New: shown on Home, Recent and Templates.
    home_tools: gtk::Box,
    new_button: gtk::Button,
    /// The front file's readout, view buttons, zoom, Find, Inspector and Export.
    doc_tools: gtk::Box,
    /// Each tab's view buttons and Inspector toggle, shown while it's in front.
    views_slot: gtk::Box,
    toggles_slot: gtk::Box,
    views: Vec<(Tab, gtk::Box)>,
    toggles: Vec<(Tab, gtk::ToggleButton)>,
    readout: gtk::Label,
    zoom: gtk::Box,
    zoom_level: gtk::Button,
    export_label: gtk::Label,
    search: gtk::SearchEntry,
    search_box: gtk::Overlay,
    search_hint: gtk::Label,
    finds: Vec<(Tab, FindBar)>,
    fullscreen: bool,
}

impl Ui {
    fn refresh_nav(&self) {
        self.nav.set_visible(!self.fullscreen && !FOCUS.with(|f| f.get()));
    }
}

#[derive(Clone)]
struct FindBar {
    bar: gtk::Box,
    query: gtk::SearchEntry,
    with: gtk::Entry,
    case: gtk::ToggleButton,
    count: gtk::Label,
}

thread_local! {
    static UI: RefCell<Option<Rc<RefCell<Ui>>>> = const { RefCell::new(None) };
    static NARROW: Cell<bool> = const { Cell::new(false) };
    /// Set once unsaved changes are dealt with, so closing goes through.
    static FORCE_CLOSE: Cell<bool> = const { Cell::new(false) };
    static TOAST: RefCell<Option<gtk::Widget>> = const { RefCell::new(None) };
}

fn ui() -> Option<Rc<RefCell<Ui>>> {
    UI.with(|u| u.borrow().clone())
}

/// Show the window (building it the first time), on Home.
pub fn present(app: &gtk::Application) {
    if let Some(ui) = ui() {
        let window = ui.borrow().window.clone();
        window.present();
        return;
    }
    theme::install();
    install_icons();
    build(app);
    navigate("home");
    if let Some(ui) = ui() {
        ui.borrow().window.present();
    }
    doc::start_recovery_timer();
    if prefs::take_broken() {
        toast("Your settings file couldn't be read, so defaults are in use. The old file is kept as settings.toml.bak.");
    }
}

/// Our symbolic icons (Lucide, see data/icons/LICENSE; strokes turned into fills so GTK can recolour them), written to the cache once
/// and added to the icon search path.
fn install_icons() {
    macro_rules! icons {
        ($($name:literal),* $(,)?) => {
            &[$(($name, include_str!(concat!("../data/icons/", $name, ".svg")))),*]
        };
    }
    const ICONS: &[(&str, &str)] = icons![
        "square-check",
        "film",
        "music",
        "crown",
        "book-marked",
        "between-horizontal-start",
        "grid-2x2",
        "wrap-text",
        "sigma",
        "rotate-cw",
        "rotate-ccw",
        "triangle",
        "diamond",
        "star",
        "hexagon",
        "arrow-big-right",
        "squircle",
        "pen-line",
        "layers-2",
        "file-pen-line",
        "check-check",
        "x-circle",
        "table-cells-merge",
        "message-square-text",
        "highlighter",
        "group",
        "ungroup",
        "align-horizontal-distribute-center",
        "align-vertical-distribute-center",
        "align-start-vertical",
        "align-center-vertical",
        "align-end-vertical",
        "align-start-horizontal",
        "align-center-horizontal",
        "align-end-horizontal",
        "sparkles",
        "wand-sparkles",
        "layers",
        "list-tree",
        "columns-2",
        "separator-horizontal",
        "superscript",
        "subscript",
        "link",
        "unfold-vertical",
        "omega",
        "gallery-thumbnails",
        "paintbrush",
        "message-square-plus",
        "align-center",
        "align-left",
        "align-right",
        "arrow-left",
        "arrow-right",
        "arrow-up-narrow-wide",
        "bold",
        "chart-area",
        "chart-column",
        "chart-line",
        "chart-pie",
        "check",
        "chevron-down",
        "clipboard-paste",
        "clock-3",
        "copy",
        "currency",
        "decimal",
        "ellipsis",
        "file-down",
        "file-plus",
        "file-spreadsheet",
        "files",
        "file-text",
        "filter",
        "folder-open",
        "house",
        "image",
        "italic",
        "layout-grid",
        "list-ordered",
        "list",
        "palette",
        "panel-left",
        "percent",
        "play",
        "presentation",
        "redo-2",
        "save",
        "scissors",
        "settings-2",
        "shapes",
        "sliders-horizontal",
        "table-2",
        "type",
        "undo-2",
        "x",
        "underline",
        "heading",
        "printer",
        "search",
        "replace",
        "case-sensitive",
        "baseline",
        "paint-bucket",
        "snowflake",
        "calendar",
        "plus",
        "trash-2",
        "monitor",
        "square",
        "circle",
        "text-cursor",
        "notebook-pen",
        "mail",
        "chef-hat",
        "plane",
        "piggy-bank",
        "package",
        "party-popper",
        "book-open",
        "file-type",
        "arrow-down",
        "arrow-up",
        "grip-vertical",
        "eraser",
        "strikethrough",
        "indent-increase",
        "indent-decrease",
        "minus",
        "table-rows-split",
        "ruler",
    ];
    let dir = crate::paths::cache_dir().join("icons");
    for (name, svg) in ICONS {
        let path = dir.join(format!("nxo-{name}-symbolic.svg"));
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*svg) {
            let _ = crate::cmd::atomic_write(&path, svg);
        }
    }
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display).add_search_path(&dir);
    }
}

/// A kind's icon, in the kind's colour.
pub fn kind_icon(kind: Kind) -> gtk::Image {
    let image = gtk::Image::from_icon_name(kind.icon());
    image.add_css_class("kind-icon");
    image.add_css_class(&kind.color_class());
    image
}

/// The New menu: a blank file of each kind, or the templates.
fn new_entries() -> Vec<nyx_ui::menu::Entry> {
    use nyx_ui::menu::{item, separator};
    let mut out: Vec<nyx_ui::menu::Entry> = Kind::ALL
        .into_iter()
        .map(|k| {
            item(k.title(), move || {
                doc::new_of(k, None);
            })
            .icon(k.icon())
        })
        .collect();
    out.push(separator());
    out.push(item("From a template…", || navigate("templates")).icon("nxo-sparkles-symbolic"));
    out
}

/// A labelled button for the header: an icon, then the label.
fn header_button(icon: &str, label: &str, class: &str) -> (gtk::Box, gtk::Label) {
    let face = widgets::hbox(6);
    face.append(&gtk::Image::from_icon_name(icon));
    let l = gtk::Label::new(Some(label));
    face.append(&l);
    if !class.is_empty() {
        face.add_css_class(class);
    }
    (face, l)
}

fn nav_divider(section: &str) -> gtk::Separator {
    let bar = gtk::Separator::new(gtk::Orientation::Horizontal);
    bar.add_css_class("nav-divider");
    bar.add_css_class("compact-show");
    bar.add_css_class(&format!("section-{section}"));
    bar.set_visible(false);
    bar
}

fn nav_group(list: &gtk::Box, title: &str, section: &str) {
    list.append(&nav_divider(section));
    let row = sidebar::group_label(title, false);
    sidebar::set_section(&row, section);
    list.append(&row);
}

fn build(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::builder().application(app).title("Office").default_width(1120).default_height(800).build();
    window.add_css_class("office-window");
    // No client-side titlebar: Hyprland manages the window.
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    nyx_ui::keys::install(&window, nyx_ui::keys::Common { shortcuts: Some(show_shortcuts), ..Default::default() });
    window.set_icon_name(Some(app::APP_ID));

    // ----- Sidebar -----
    let nav = gtk::Box::new(gtk::Orientation::Vertical, 0);
    nav.add_css_class("settings-navigation");
    nav.set_hexpand(false);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let home_item = sidebar::nav_item("nxo-house-symbolic", "Home", "Start something new, or pick up a recent file");
    home_item.button.connect_clicked(|_| {
        home::set_filter(None);
        navigate("home");
    });
    list.append(&home_item.button);
    let recent_item = sidebar::nav_item("nxo-clock-3-symbolic", "Recent", "Every file you opened or saved");
    recent_item.button.connect_clicked(|_| {
        home::set_filter(None);
        navigate("recent");
    });
    list.append(&recent_item.button);
    nav_group(&list, "Kinds", sidebar::ACCENT_SECTION);
    let kinds = Kind::ALL.map(|k| {
        let item = sidebar::nav_item(k.icon(), k.plural(), &format!("Your recent {}s", k.noun()));
        // The sidebar's own icon colour wins over a swatch's: the swatch goes on a box
        // around the icon, which takes its colour from there.
        if let Some(content) = item.button.child().and_downcast::<gtk::Box>()
            && let Some(icon) = content.first_child()
        {
            content.remove(&icon);
            icon.add_css_class("kind-nav-icon");
            let tint = widgets::hbox(0);
            tint.add_css_class(&k.color_class());
            tint.set_valign(gtk::Align::Center);
            tint.append(&icon);
            content.prepend(&tint);
        }
        item.button.connect_clicked(move |_| {
            home::set_filter(Some(k));
            navigate("recent");
        });
        list.append(&item.button);
        item
    });
    nav_group(&list, "Templates", "swatch-green");
    let templates_item = sidebar::nav_item("nxo-sparkles-symbolic", "All templates", "Letters, budgets, slides and more to start from");
    templates_item.button.connect_clicked(|_| navigate("templates"));
    list.append(&templates_item.button);

    let nav_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vexpand(true)
        .child(&list)
        .build();
    nav.append(&nav_scroll);
    let foot = widgets::hbox(6);
    foot.add_css_class("nav-foot");
    foot.add_css_class("compact-hide");
    let tick = gtk::Image::from_icon_name("nxo-check-symbolic");
    tick.add_css_class("foot-ok");
    foot.append(&tick);
    let foot_label = widgets::label("", "foot-text");
    foot.append(&foot_label);
    nav.append(&foot);

    // ----- Pages -----
    let stack = gtk::Stack::new();
    stack.add_css_class("settings-content");
    stack.add_css_class("office-pages");
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(if prefs::get().reduce_motion { 0 } else { 160 });
    for name in PAGES {
        stack.add_named(&home::build(name), Some(name));
    }

    // ----- Tabs -----
    let tabs_bar = widgets::hbox(0);
    tabs_bar.add_css_class("office-tabs");
    let tabs = widgets::hbox(2);
    tabs_bar.append(&tabs);
    let plus = gtk::Button::from_icon_name("nxo-plus-symbolic");
    plus.add_css_class("office-tab-new");
    plus.set_tooltip_text(Some("New document, spreadsheet or presentation"));
    plus.update_property(&[gtk::accessible::Property::Label("New")]);
    plus.set_valign(gtk::Align::Center);
    plus.connect_clicked(|b| nyx_ui::menu::popup_below(b, new_entries()));
    tabs_bar.append(&plus);
    let page_box = widgets::vbox(0);
    page_box.append(&tabs_bar);
    page_box.append(&nyx_ui::frame::pane_line(gtk::Orientation::Horizontal));
    page_box.append(&stack);

    // ----- Header -----
    let header = chrome::app_header(&window, "nxo-file-text-symbolic", "Office");
    let side_toggle = widgets::bar_button("sidebar-show-symbolic", "Collapse or expand the sidebar (Ctrl+Shift+B)");
    side_toggle.connect_clicked(|_| toggle_sidebar());
    header.start.append(&side_toggle);
    let doc_title = widgets::hbox(6);
    doc_title.add_css_class("header-doc");
    doc_title.append(&widgets::label("·", "header-doc-sep"));
    let doc_name = widgets::label("", "header-doc-name");
    doc_name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    doc_name.set_max_width_chars(36);
    doc_title.append(&doc_name);
    let doc_dot = widgets::hbox(0);
    doc_dot.add_css_class("unsaved-dot");
    doc_dot.set_valign(gtk::Align::Center);
    doc_dot.set_tooltip_text(Some("Not saved"));
    doc_title.append(&doc_dot);
    doc_title.set_visible(false);
    header.start.append(&doc_title);

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search documents, sheets and slides"));
    search.update_property(&[gtk::accessible::Property::Label("Search recent files")]);
    search.add_css_class("bar-search");
    search.set_width_chars(10);
    search.set_max_width_chars(40);
    search.connect_search_changed(|e| home::set_query(e.text().trim()));
    search.connect_stop_search(|e| e.set_text(""));
    let search_box = gtk::Overlay::new();
    search_box.set_child(Some(&search));
    let hint = widgets::label("Ctrl F", "key-hint");
    hint.set_halign(gtk::Align::End);
    hint.set_valign(gtk::Align::Center);
    hint.set_margin_end(10);
    hint.set_can_target(false);
    search_box.add_overlay(&hint);
    let h = hint.clone();
    search.connect_changed(move |s| h.set_visible(s.text().is_empty() && !narrow()));
    header.center.append(&search_box);

    header.end.set_spacing(4);
    let home_tools = widgets::hbox(6);
    let open = gtk::Button::new();
    open.add_css_class("flat");
    open.add_css_class("header-open");
    open.set_child(Some(&header_button("nxo-folder-open-symbolic", "Open…", "").0));
    open.set_tooltip_text(Some("Open a file (Ctrl+O)"));
    open.set_valign(gtk::Align::Center);
    open.connect_clicked(|_| doc::open_dialog());
    home_tools.append(&open);
    let new_button = gtk::Button::new();
    new_button.add_css_class("new-button");
    let (face, _) = header_button("nxo-plus-symbolic", "New", "");
    let chev = gtk::Image::from_icon_name("nxo-chevron-down-symbolic");
    chev.add_css_class("dd-chev");
    face.append(&chev);
    new_button.set_child(Some(&face));
    new_button.set_tooltip_text(Some("New document, spreadsheet or presentation (Ctrl+N)"));
    new_button.set_valign(gtk::Align::Center);
    new_button.connect_clicked(|b| nyx_ui::menu::popup_below(b, new_entries()));
    home_tools.append(&new_button);
    header.end.append(&home_tools);

    let doc_tools = widgets::hbox(4);
    let readout = widgets::label("", "header-status");
    readout.set_ellipsize(gtk::pango::EllipsizeMode::End);
    readout.set_max_width_chars(30);
    readout.set_margin_end(4);
    doc_tools.append(&readout);
    let views_slot = widgets::hbox(0);
    doc_tools.append(&views_slot);
    let focus = widgets::bar_button("view-conceal-symbolic", "Focus mode: just the document (Ctrl+Shift+F)");
    focus.connect_clicked(|_| toggle_focus());
    doc_tools.append(&focus);
    let full = widgets::bar_button("view-fullscreen-symbolic", "Full screen (F11)");
    full.connect_clicked(|_| toggle_fullscreen());
    doc_tools.append(&full);
    let (zoom, zoom_level) = zoom_control();
    doc_tools.append(&zoom);
    doc_tools.append(&find_button());
    let toggles_slot = widgets::hbox(0);
    doc_tools.append(&toggles_slot);
    let export = gtk::Button::new();
    export.add_css_class("header-export");
    let (face, export_label) = header_button("nxo-file-down-symbolic", "Export PDF", "");
    export.set_child(Some(&face));
    export.set_tooltip_text(Some("Export as PDF (Ctrl+E)"));
    export.set_valign(gtk::Align::Center);
    export.set_margin_start(8);
    export.connect_clicked(|_| doc::export_pdf_dialog());
    doc_tools.append(&export);
    doc_tools.set_visible(false);
    header.end.append(&doc_tools);
    header.end.append(&chrome::settings_button(settings_dialog::open));

    let frame = nyx_ui::frame::AppFrame::new(&header.root);
    frame.set_sidebar(&nav);
    frame.set_page(&page_box);
    frame.set_flush(true);
    let overlay = frame.install(&window);

    install_keys(&window);
    window.connect_close_request(|w| {
        if FORCE_CLOSE.with(|f| f.get()) || !doc::any_dirty() {
            doc::clear_all_recovery();
            return glib::Propagation::Proceed;
        }
        let w = w.clone();
        doc::guard_all(move || {
            FORCE_CLOSE.with(|f| f.set(true));
            w.close();
        });
        glib::Propagation::Stop
    });

    // Drop a file anywhere on the window to open it.
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    drop.connect_drop(|_, value, _, _| {
        let Ok(list) = value.get::<gdk::FileList>() else { return false };
        let files: Vec<_> = list.files().into_iter().filter_map(|f| f.path()).filter(|p| app::opens_any(p)).collect();
        if files.is_empty() {
            toast("Office opens Word, Excel, PowerPoint, OpenDocument, Markdown, CSV and text files.");
            return false;
        }
        for p in files {
            doc::open(p);
        }
        true
    });
    window.add_controller(drop);

    // Narrow windows (a tiled half-screen) get an icon-only sidebar.
    let apply_width = {
        let nav = nav.clone();
        let last_width = Rc::new(Cell::new(0));
        move |w: &gtk::ApplicationWindow| {
            let allocated = if w.width() > 0 { w.width() } else { w.default_width() };
            let width = w.surface().map(|s| s.width()).filter(|width| *width > 0).map_or(allocated, |width| allocated.min(width));
            let half_screen = w.surface()
                .and_then(|surface| surface.display().monitor_at_surface(&surface))
                .is_some_and(|monitor| width <= monitor.geometry().width() / 2 + 32);
            let narrow = width > 0 && (width < 980 || half_screen);
            let resized = last_width.replace(width) != width;
            settings_dialog::fit(w);
            if narrow && resized {
                close_narrow_panels();
            }
            if narrow == NARROW.with(|n| n.get()) && nav.has_css_class("sized") {
                return;
            }
            nav.add_css_class("sized");
            set_narrow(narrow);
            refresh_compact();
        }
    };
    let aw = apply_width.clone();
    window.connect_default_width_notify(move |w| aw(w));
    let aw = apply_width.clone();
    window.connect_realize(move |w| {
        aw(w);
        if let Some(surface) = w.surface() {
            let (aw, w) = (aw.clone(), w.clone());
            // The compositor can shrink a tiled surface while GTK keeps a
            // wider minimum allocation for the open panes. Widget resize
            // signals then do not fire, but the surface width does change.
            surface.connect_width_notify(move |_| {
                let (aw, w) = (aw.clone(), w.clone());
                glib::idle_add_local_once(move || aw(&w));
            });
        }
    });
    // Tiled windows are resized by the compositor without touching the default
    // size: an invisible layer over the whole window reports each real size change.
    let probe = gtk::DrawingArea::new();
    probe.set_can_target(false);
    probe.set_can_focus(false);
    overlay.add_overlay(&probe);
    overlay.set_measure_overlay(&probe, false);
    {
        let (aw, w2) = (apply_width.clone(), window.clone());
        probe.connect_resize(move |_, _, _| {
            let (aw, w2) = (aw.clone(), w2.clone());
            glib::idle_add_local_once(move || aw(&w2));
        });
    }

    let state = Ui {
        window: window.clone(),
        stack,
        nav,
        items: Nav { home: home_item, recent: recent_item, kinds, templates: templates_item, list, groups: Vec::new(), foot, foot_label },
        current: String::new(),
        overlay,
        chrome: vec![header.root.clone().upcast(), tabs_bar.clone().upcast()],
        tabs,
        tabs_bar,
        order: Vec::new(),
        doc_title,
        doc_name,
        doc_dot,
        home_tools,
        new_button,
        doc_tools,
        views_slot,
        toggles_slot,
        views: Vec::new(),
        toggles: Vec::new(),
        readout,
        zoom,
        zoom_level,
        export_label,
        search,
        search_box,
        search_hint: hint,
        finds: Vec::new(),
        fullscreen: false,
    };
    UI.with(|u| *u.borrow_mut() = Some(Rc::new(RefCell::new(state))));

    // The tabs, the header, the sidebar and the window title follow the files.
    doc::subscribe(&window, |c| match c {
        doc::Change::Opened | doc::Change::Dirty | doc::Change::Closed => refresh_all(),
        doc::Change::Status => refresh_status(),
    });
}

/// Give a new tab (the one being worked on) its page, header tools, Inspector
/// toggle, sidebar group and find bar. Shown once a file is in it.
pub fn add_tab(tab: Tab) {
    let Some(ui) = ui() else { return };
    let Some(editor) = doc::editor_in(tab) else { return };
    let doc_page = widgets::vbox(0);
    doc_page.add_css_class("document-page");
    let editor_widget = editor.widget();
    editor_widget.set_vexpand(true);
    let find = find_bar(tab);
    // The find bar sits under the editor's toolbar row (its first child).
    if let Some(bx) = editor_widget.downcast_ref::<gtk::Box>()
        && let Some(toolbar) = bx.first_child()
    {
        bx.insert_child_after(&find.bar, Some(&toolbar));
    } else {
        doc_page.append(&find.bar);
    }
    doc_page.append(&editor_widget);
    // Ctrl+scroll zooms, before the editor's own scrolling sees it.
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    // A touchpad sends many small steps: add them up to a wheel click's worth.
    let pending = Cell::new(0.0);
    wheel.connect_scroll(move |c, _, dy| {
        if !c.current_event_state().contains(gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let step = if c.unit() == gdk::ScrollUnit::Surface { dy / 40.0 } else { dy };
        let total = pending.get() + step;
        if total.abs() >= 1.0 {
            app::within(tab, || doc::zoom_step(total < 0.0));
            pending.set(0.0);
        } else {
            pending.set(total);
        }
        glib::Propagation::Stop
    });
    doc_page.add_controller(wheel);
    mark_narrow(doc_page.upcast_ref(), narrow());
    editor.set_zoom(doc::zoom_of(Some(tab)));

    let views = widgets::hbox(2);
    for w in STATUS_TOOLS.with(|t| take_of(&mut t.borrow_mut(), tab)).map(|(_, w)| w).unwrap_or_default() {
        views.append(&w);
    }
    views.set_visible(false);
    let toggle = chrome_of(tab).map(|(_, inspector)| {
        let toggle = inspector.toggle_button().clone();
        toggle.add_css_class("bar-button");
        toggle.set_visible(false);
        toggle
    });
    let group = editor.sidebar().map(|group| {
        let section = widgets::vbox(0);
        section.add_css_class("editor-group");
        let head = match group.add {
            Some((tip, act)) => sidebar::group_label_with_add(&group.title, false, &tip, move || act()),
            None => sidebar::group_label(&group.title, false),
        };
        sidebar::set_section(&head, tab.kind.swatch_section());
        section.append(&head);
        group.widget.add_css_class("compact-hide");
        section.append(&group.widget);
        section.set_visible(false);
        section
    });

    let mut u = ui.borrow_mut();
    u.stack.add_named(&doc_page, Some(&tab.page()));
    u.views_slot.append(&views);
    u.views.push((tab, views));
    if let Some(toggle) = toggle {
        u.toggles_slot.append(&toggle);
        u.toggles.push((tab, toggle));
    }
    if let Some(section) = group {
        u.items.list.append(&section);
        // The sidebar's icon-rail state is set by walking its widgets: a group added later needs it too.
        let compact = compact();
        sidebar::set_compact(&section, compact);
        section.remove_css_class("compact");
        set_compact_show(section.upcast_ref(), compact);
        u.items.groups.push((tab, section));
    }
    u.finds.push((tab, find));
}

/// Take a closed tab's page and tools away. When it was in front, its neighbour
/// (or Home) comes forward first.
pub fn remove_tab(tab: Tab) {
    let Some(ui) = ui() else { return };
    if app::front() == Some(tab) {
        let order = ui.borrow().order.clone();
        let at = order.iter().position(|o| *o == tab).unwrap_or(0);
        let next = order.iter().take(at).rev().chain(order.iter().skip(at + 1)).copied().find(|o| doc::is_open_in(*o));
        match next {
            Some(n) => show_document(n),
            None => navigate("home"),
        }
    }
    {
        let mut u = ui.borrow_mut();
        u.order.retain(|t| *t != tab);
        if let Some(page) = u.stack.child_by_name(&tab.page()) {
            u.stack.remove(&page);
        }
        if let Some((_, views)) = take_of(&mut u.views, tab) {
            u.views_slot.remove(&views);
        }
        if let Some((_, toggle)) = take_of(&mut u.toggles, tab) {
            u.toggles_slot.remove(&toggle);
        }
        if let Some((_, section)) = take_of(&mut u.items.groups, tab) {
            u.items.list.remove(&section);
        }
        take_of(&mut u.finds, tab);
    }
    CHROME.with(|c| take_of(&mut c.borrow_mut(), tab));
    STATUS_TOOLS.with(|t| take_of(&mut t.borrow_mut(), tab));
    refresh_all();
}

/// Take `tab`'s entry out of a per-tab list.
fn take_of<T>(list: &mut Vec<(Tab, T)>, tab: Tab) -> Option<(Tab, T)> {
    let at = list.iter().position(|(t, _)| *t == tab)?;
    Some(list.remove(at))
}

impl Kind {
    /// The section (sidebar heading colour) of the kind's own group.
    fn swatch_section(self) -> &'static str {
        match self {
            Kind::Write => "swatch-blue",
            Kind::Sheets => "swatch-green",
            Kind::Present => "swatch-coral",
        }
    }
}

/// An editor's command bar and Inspector.
type Chrome = (Rc<CommandBar>, Rc<Inspector>);

thread_local! {
    static CHROME: RefCell<Vec<(Tab, Chrome)>> = const { RefCell::new(Vec::new()) };
    /// View buttons each editor puts in the header while it's built, before its tab has its tools.
    static STATUS_TOOLS: RefCell<Vec<(Tab, Vec<gtk::Widget>)>> = const { RefCell::new(Vec::new()) };
}

fn chrome_of(tab: Tab) -> Option<Chrome> {
    CHROME.with(|c| c.borrow().iter().find(|(t, _)| *t == tab).map(|(_, c)| c.clone()))
}

/// The editor's command bar and Inspector (the editor being built is the tab being
/// worked on): the window finishes the bar and remembers whether the Inspector is open.
pub fn attach_chrome(bar: &Rc<CommandBar>, inspector: &Rc<Inspector>) {
    // The file's name is in the header, not a title on the bar.
    bar.title_button().set_visible(false);
    bar.set_title("", "", false);
    let kind = app::kind();
    let tab = app::tab();
    let id = kind.id();
    let open = prefs::get().inspector.get(id).copied().unwrap_or(kind != Kind::Sheets);
    inspector.set_open(open);
    inspector.connect_open(move |on| {
        prefs::update(|p| {
            p.inspector.insert(id.to_string(), on);
        })
    });
    // Before the Inspector floats over the document, the sidebar goes to icons
    // to make room for it (not saved as a setting; closing it brings it back).
    let me = Rc::downgrade(inspector);
    inspector.connect_floating(move |floating| {
        if floating && tab.is_some() && app::front() == tab && me.upgrade().is_some_and(|i| i.is_open()) {
            make_room(true);
        }
    });
    let me = Rc::downgrade(inspector);
    inspector.connect_open(move |open| {
        if tab.is_none() || app::front() != tab {
            return;
        }
        if open && me.upgrade().is_some_and(|i| i.is_floating()) {
            make_room(true);
        } else if !open {
            make_room(false);
        }
    });
    if let Some(tab) = tab {
        CHROME.with(|c| c.borrow_mut().push((tab, (bar.clone(), inspector.clone()))));
    }
}

thread_local! {
    /// The sidebar shows only icons to make room for the Inspector.
    static ROOM: Cell<bool> = const { Cell::new(false) };
}

fn make_room(on: bool) {
    if ROOM.with(|r| r.get()) == on {
        return;
    }
    if on && (narrow() || prefs::get().sidebar_collapsed) {
        return;
    }
    ROOM.with(|r| r.set(on));
    refresh_compact();
}

/// The front file's Inspector.
fn inspector() -> Option<Rc<Inspector>> {
    app::front().and_then(chrome_of).map(|(_, i)| i)
}

fn inspectors() -> Vec<Rc<Inspector>> {
    CHROME.with(|c| c.borrow().iter().map(|(_, (_, i))| i.clone()).collect())
}

/// Ctrl+Alt+I (and Ctrl+F1): show or hide the Inspector.
fn toggle_inspector() {
    if let Some(i) = inspector() {
        i.toggle();
    }
}

/// A view button for the header (the chart pane, the sorter), for the tab being built.
/// The editor adds them while it builds, before its tab has its tools.
pub fn status_tool(w: &impl IsA<gtk::Widget>) {
    let Some(tab) = app::tab() else { return };
    STATUS_TOOLS.with(|t| {
        let mut t = t.borrow_mut();
        match t.iter_mut().find(|(x, _)| *x == tab) {
            Some((_, list)) => list.push(w.clone().upcast()),
            None => t.push((tab, vec![w.clone().upcast()])),
        }
    });
}

thread_local! {
    /// In focus mode.
    static FOCUS: Cell<bool> = const { Cell::new(false) };
}

/// Focus mode (Ctrl+Shift+F): just the document. The Inspector, the sidebar and the
/// tabs hide until it's turned off again; the header stays, to turn it off.
pub fn toggle_focus() {
    let Some(ui) = ui() else { return };
    let on = !FOCUS.with(|f| f.get());
    FOCUS.with(|f| f.set(on));
    for i in inspectors() {
        i.suspend(on);
    }
    let u = ui.borrow();
    u.refresh_nav();
    u.tabs_bar.set_visible(!on && !u.fullscreen);
}

/// Find, for the header.
pub fn find_button() -> gtk::Button {
    let b = widgets::bar_button("nxo-search-symbolic", "Find and replace (Ctrl+F, Ctrl+H)");
    b.connect_clicked(|_| find_or_search(false));
    b
}

/// Ctrl+F: the find bar while a file is in front; otherwise search recent files.
fn find_or_search(replace: bool) {
    if app::front().is_some() {
        show_find(replace);
        return;
    }
    let Some(ui) = ui() else { return };
    let search = ui.borrow().search.clone();
    search.grab_focus();
}

// ---------- Tabs ----------

/// One tab: its icon and name, a dot while unsaved, and Close (shown on the tab in
/// front and under the pointer; over the dot, which it replaces while hovered).
fn tab(file: Option<Tab>, active: bool) -> gtk::Box {
    let item = widgets::hbox(0);
    item.add_css_class("office-tab");
    if active {
        item.add_css_class("active");
    }
    let main = gtk::Button::new();
    main.add_css_class("office-tab-main");
    let content = widgets::hbox(7);
    let title = match file {
        Some(t) => {
            content.append(&kind_icon(t.kind));
            doc::name_in(t)
        }
        None => {
            content.append(&gtk::Image::from_icon_name("nxo-house-symbolic"));
            "Home".to_string()
        }
    };
    let label = gtk::Label::new(Some(&title));
    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    label.set_max_width_chars(24);
    content.append(&label);
    main.set_child(Some(&content));
    main.set_tooltip_text(Some(&match file.and_then(doc::path_in) {
        Some(p) => crate::paths::pretty(&p),
        None => title.clone(),
    }));
    main.update_property(&[gtk::accessible::Property::Label(&title)]);
    main.connect_clicked(move |_| match file {
        Some(t) => show_document(t),
        None => navigate("home"),
    });
    item.append(&main);
    let Some(k) = file else { return item };
    let dirty = doc::dirty_in(k);
    if dirty {
        item.add_css_class("dirty");
    }
    let end = gtk::Overlay::new();
    end.set_valign(gtk::Align::Center);
    let close = gtk::Button::from_icon_name("nxo-x-symbolic");
    close.add_css_class("office-tab-close");
    close.set_tooltip_text(Some("Close (Ctrl+W)"));
    close.update_property(&[gtk::accessible::Property::Label(&format!("Close {title}"))]);
    close.connect_clicked(move |_| doc::close(k));
    end.set_child(Some(&close));
    let dot = widgets::hbox(0);
    dot.add_css_class("unsaved-dot");
    dot.add_css_class("office-tab-dot");
    dot.set_halign(gtk::Align::Center);
    dot.set_valign(gtk::Align::Center);
    dot.set_can_target(false);
    dot.set_visible(dirty);
    end.add_overlay(&dot);
    item.append(&end);
    let middle = gtk::GestureClick::new();
    middle.set_button(gdk::BUTTON_MIDDLE);
    middle.connect_released(move |g, _, _, _| {
        g.set_state(gtk::EventSequenceState::Claimed);
        doc::close(k);
    });
    item.add_controller(middle);
    item
}

fn refresh_tabs() {
    let Some(ui) = ui() else { return };
    let mut u = ui.borrow_mut();
    u.order.retain(|k| doc::is_open_in(*k));
    while let Some(c) = u.tabs.first_child() {
        u.tabs.remove(&c);
    }
    let front = app::front();
    u.tabs.append(&tab(None, front.is_none() && u.current == "home"));
    for k in u.order.clone() {
        u.tabs.append(&tab(Some(k), front == Some(k)));
    }
}

/// Bring the tab's file to the front (Home, when the tab has none).
pub fn show_document(tab: Tab) {
    if !doc::is_open_in(tab) {
        navigate("home");
        return;
    }
    let Some(ui) = ui() else { return };
    {
        let mut u = ui.borrow_mut();
        if !u.order.contains(&tab) {
            u.order.push(tab);
        }
    }
    show_page(&tab.page());
    if let Some(e) = doc::editor_in(tab) {
        e.focus();
    }
}

/// The front file of `kind` (Home, when none is open).
pub fn show_kind(kind: Kind) {
    match doc::front_of(kind) {
        Some(t) => show_document(t),
        None => navigate("home"),
    }
}

/// Ctrl+Tab and Ctrl+Shift+Tab: the next or previous tab, Home among them.
fn cycle_tabs(forward: bool) {
    let Some(ui) = ui() else { return };
    let order: Vec<Option<Tab>> =
        std::iter::once(None).chain(ui.borrow().order.iter().copied().filter(|t| doc::is_open_in(*t)).map(Some)).collect();
    let at = order.iter().position(|k| *k == app::front()).unwrap_or(0);
    let n = order.len();
    match order[if forward { (at + 1) % n } else { (at + n - 1) % n }] {
        Some(t) => show_document(t),
        None => navigate("home"),
    }
}

// ---------- Header, sidebar and title ----------

/// Everything that follows the files: the tabs, the header, the sidebar, the title.
fn refresh_all() {
    refresh_tabs();
    refresh_header();
    refresh_sidebar();
    refresh_status();
    let Some(w) = window() else { return };
    w.set_title(Some(&match app::front() {
        Some(t) => format!("{}{} — Office", if doc::dirty_in(t) { "• " } else { "" }, doc::title_in(t)),
        None => "Office".to_string(),
    }));
}

fn refresh_header() {
    let Some(ui) = ui() else { return };
    let u = ui.borrow();
    let front = app::front();
    u.home_tools.set_visible(front.is_none());
    u.search_box.set_visible(front.is_none());
    u.doc_tools.set_visible(front.is_some());
    for (t, views) in &u.views {
        views.set_visible(front == Some(*t));
    }
    for (t, toggle) in &u.toggles {
        toggle.set_visible(front == Some(*t));
    }
    let page_name = match u.current.as_str() {
        "recent" => Some("Recent".to_string()),
        "templates" => Some("Templates".to_string()),
        _ => None,
    };
    match front.map(doc::name_in).or(page_name) {
        Some(name) => {
            u.doc_name.set_text(&name);
            u.doc_title.set_visible(true);
        }
        None => u.doc_title.set_visible(false),
    }
    u.doc_dot.set_visible(front.is_some_and(doc::dirty_in));
    u.zoom.set_visible(front.is_some() && !narrow());
    u.export_label.set_text(if narrow() { "PDF" } else { "Export PDF" });
}

fn refresh_sidebar() {
    let Some(ui) = ui() else { return };
    let u = ui.borrow();
    let n = &u.items;
    let filter = home::filter();
    let page = u.current.as_str();
    n.home.set_active(page == "home");
    n.recent.set_active(page == "recent" && filter.is_none());
    n.templates.set_active(page == "templates");
    let all = crate::recent::all();
    n.recent.set_count(Some(all.len() as u64));
    for k in Kind::ALL {
        n.kinds[k.index()].set_active(page == "recent" && filter == Some(k));
        n.kinds[k.index()].set_count(Some(all.iter().filter(|(kind, _)| *kind == k).count() as u64));
    }
    for (t, g) in &n.groups {
        g.set_visible(app::front() == Some(*t));
    }
    let templates: usize = Kind::ALL.iter().map(|k| doc::templates(*k).len()).sum();
    n.templates.set_count(Some(templates as u64));
    let p = prefs::get();
    let foot = if p.autosave {
        Some("Saves on its own")
    } else if p.recovery {
        Some("Keeps a copy of unsaved work")
    } else {
        None
    };
    n.foot_label.set_text(foot.unwrap_or(""));
    n.foot.set_visible(foot.is_some() && !compact());
}

/// Saving by itself or keeping unsaved work was turned on or off: the sidebar's foot says which.
pub fn files_prefs_changed() {
    refresh_sidebar();
}

fn refresh_status() {
    let Some(ui) = ui() else { return };
    let u = ui.borrow();
    let text = match app::front().and_then(|t| Some((t, doc::editor_in(t)?))) {
        Some((t, e)) if !e.own_readout() => app::within(t, || e.status()),
        _ => String::new(),
    };
    u.readout.set_text(&text);
    u.readout.set_visible(!text.is_empty() && !narrow());
    if let Some(t) = app::front() {
        u.zoom_level.set_label(&format!("{:.0}%", doc::zoom_of(Some(t)) * 100.0));
    }
}

// ---------- Find and replace ----------

fn find_bar(tab: Tab) -> FindBar {
    let bar = widgets::hbox(6);
    bar.add_css_class("find-bar");
    bar.set_visible(false);
    let query = gtk::SearchEntry::new();
    query.set_placeholder_text(Some("Find"));
    query.set_width_chars(18);
    query.set_hexpand(true);
    let case = gtk::ToggleButton::new();
    case.set_icon_name("nxo-case-sensitive-symbolic");
    case.add_css_class("search-option");
    case.set_tooltip_text(Some("Match case"));
    let prev = widgets::bar_button("nxo-arrow-up-symbolic", "Previous match (Shift+Enter)");
    let next = widgets::bar_button("nxo-arrow-down-symbolic", "Next match (Enter)");
    let count = widgets::label("", "find-count");
    count.add_css_class("dim");
    let with = gtk::Entry::new();
    with.set_placeholder_text(Some("Replace with"));
    with.set_width_chars(16);
    with.set_hexpand(true);
    let one = gtk::Button::with_label("Replace");
    one.set_tooltip_text(Some("Replace this match and find the next (Ctrl+H)"));
    let all = gtk::Button::with_label("All");
    all.set_tooltip_text(Some("Replace every match"));
    let close = widgets::bar_button("window-close-symbolic", "Close (Esc)");
    bar.append(&query);
    bar.append(&case);
    bar.append(&prev);
    bar.append(&next);
    bar.append(&count);
    bar.append(&with);
    bar.append(&one);
    bar.append(&all);
    bar.append(&close);
    let fb = FindBar { bar, query, with, case, count };

    let run = move |forward: bool| {
        move || {
            let Some(f) = find_of(tab) else { return };
            let q = f.query.text().to_string();
            if q.is_empty() {
                return;
            }
            if let Some(t) = doc::editor_in(tab).and_then(|e| e.find())
                && !t.find(&q, f.case.is_active(), forward)
            {
                f.count.set_text("No matches");
            }
        }
    };
    let (n, p) = (run(true), run(false));
    next.connect_clicked(move |_| n());
    prev.connect_clicked(move |_| p());
    let (n, p) = (run(true), run(false));
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, mods| {
        if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
            if mods.contains(gdk::ModifierType::SHIFT_MASK) {
                p()
            } else {
                n()
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    fb.query.add_controller(keys);
    fb.query.connect_search_changed(move |_| refresh_find(tab, true));
    fb.case.connect_toggled(move |_| refresh_find(tab, true));
    fb.query.connect_stop_search(move |_| hide_find(tab));
    close.connect_clicked(move |_| hide_find(tab));
    let replace_one = move || {
        let Some(f) = find_of(tab) else { return };
        let q = f.query.text().to_string();
        if let Some(t) = doc::editor_in(tab).and_then(|e| e.find())
            && !q.is_empty()
        {
            app::within(tab, || t.replace(&q, &f.with.text(), f.case.is_active()));
        }
        refresh_find(tab, false);
    };
    one.connect_clicked(move |_| replace_one());
    fb.with.connect_activate(move |_| replace_one());
    all.connect_clicked(move |_| {
        let Some(f) = find_of(tab) else { return };
        let q = f.query.text().to_string();
        if let Some(t) = doc::editor_in(tab).and_then(|e| e.find())
            && !q.is_empty()
        {
            let n = app::within(tab, || t.replace_all(&q, &f.with.text(), f.case.is_active()));
            toast(&format!("Replaced {}.", crate::fmt::count(n, "match", "matches")));
        }
        refresh_find(tab, false);
    });
    fb
}

fn find_of(tab: Tab) -> Option<FindBar> {
    ui().and_then(|u| u.borrow().finds.iter().find(|(t, _)| *t == tab).map(|(_, f)| f.clone()))
}

/// Recount the matches; when the query changed, jump to the first one.
fn refresh_find(tab: Tab, jump: bool) {
    let Some(f) = find_of(tab) else { return };
    let q = f.query.text().to_string();
    let Some(t) = doc::editor_in(tab).and_then(|e| e.find()) else { return };
    if q.is_empty() {
        f.count.set_text("");
        t.done();
        return;
    }
    let n = t.count(&q, f.case.is_active());
    f.count.set_text(&if n == 0 { "No matches".to_string() } else { crate::fmt::count(n, "match", "matches") });
    if jump && n > 0 {
        t.find(&q, f.case.is_active(), true);
        f.query.grab_focus();
    }
}

fn show_find(replace: bool) {
    let Some(tab) = app::front() else { return };
    let Some(f) = find_of(tab) else { return };
    f.bar.set_visible(true);
    if replace {
        f.with.grab_focus();
    } else {
        f.query.grab_focus();
        f.query.select_region(0, -1);
    }
    refresh_find(tab, false);
}

fn hide_find(tab: Tab) {
    let Some(f) = find_of(tab) else { return };
    if !f.bar.is_visible() {
        return;
    }
    f.bar.set_visible(false);
    if let Some(t) = doc::editor_in(tab).and_then(|e| e.find()) {
        t.done();
    }
    if let Some(e) = doc::editor_in(tab) {
        e.focus();
    }
}

// ---------- Zoom and shortcuts ----------

/// − 100% + in the header; the number goes back to 100%.
fn zoom_control() -> (gtk::Box, gtk::Button) {
    let b = widgets::hbox(0);
    b.add_css_class("status-zoom");
    let button = |label: &str, tip: &str, f: fn()| {
        let x = gtk::Button::with_label(label);
        x.add_css_class("status-help");
        x.set_tooltip_text(Some(tip));
        x.connect_clicked(move |_| f());
        b.append(&x);
        x
    };
    button("−", "Zoom out (Ctrl+Alt+−)", || doc::zoom_step(false));
    let level = button("100%", "Actual size (Ctrl+Alt+0)", || doc::set_zoom(1.0));
    level.add_css_class("tnum");
    button("+", "Zoom in (Ctrl+Alt+=)", || doc::zoom_step(true));
    (b, level)
}

/// Shortcuts every kind shares, grouped for the F1 list.
pub const FILE_SHORTCUTS: &[doc::Shortcut] = &[
    (&["Ctrl", "N"], "New file of the kind in front (on Home: choose the kind)"),
    (&["Ctrl", "O"], "Open a file"),
    (&["Ctrl", "S"], "Save"),
    (&["Ctrl", "Shift", "S"], "Save as (also as an Office file)"),
    (&["Ctrl", "E"], "Export as PDF"),
    (&["Ctrl", "P"], "Print"),
    (&["Ctrl", "Alt", "="], "Zoom in (Ctrl+Alt+−: out, Ctrl+Alt+0: actual size; or Ctrl+scroll)"),
];

pub const WINDOW_SHORTCUTS: &[doc::Shortcut] = &[
    (&["Ctrl", "Tab"], "Next tab (Ctrl+Shift+Tab: the one before)"),
    (&["Ctrl", "W"], "Close the tab in front (on Home: the window)"),
    (&["Ctrl", "F"], "Find in the file, or search recent files"),
    (&["Ctrl", "H"], "Find and replace"),
    (&["Ctrl", "Z"], "Undo"),
    (&["Ctrl", "Shift", "Z"], "Redo (also Ctrl+Y)"),
    (&["Ctrl", "Shift", "B"], "Collapse or expand the sidebar"),
    (&["Ctrl", ","], "Settings"),
    (&["F11"], "Fullscreen"),
    (&["Ctrl", "Alt", "I"], "Show or hide the Inspector (also Ctrl+F1)"),
    (&["Ctrl", "Shift", "F"], "Focus mode: just the document"),
    (&["Esc"], "Close the find bar or search"),
    (&["F1"], "Show these shortcuts (or ?)"),
    (&["Ctrl", "Q"], "Quit"),
];

/// Every shortcut, by group, for the F1 list and the settings card.
pub fn shortcut_groups() -> Vec<(String, Vec<doc::Shortcut>)> {
    let mut groups = vec![("Files".to_string(), FILE_SHORTCUTS.to_vec())];
    for k in Kind::ALL {
        let own = doc::shortcuts(k);
        if !own.is_empty() {
            groups.push((k.plural().to_string(), own.to_vec()));
        }
    }
    groups.push(("Window".to_string(), WINDOW_SHORTCUTS.to_vec()));
    groups
}

pub fn show_shortcuts() {
    let (dialog, card) = widgets::dialog("Keyboard shortcuts", 520);
    let content = widgets::vbox(0);
    for (i, (title, keys)) in shortcut_groups().into_iter().enumerate() {
        let h = widgets::label(&title.to_uppercase(), "group-title");
        if i == 0 {
            h.add_css_class("page-first");
        }
        content.append(&h);
        let list = widgets::vbox(0);
        list.add_css_class("group-list");
        for (k, what) in keys {
            list.append(&widgets::row(what, "", Some(widgets::key_caps(k).upcast_ref())));
        }
        content.append(&list);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        // Scrolls without a scrollbar, which would cover the key caps.
        .vscrollbar_policy(gtk::PolicyType::External)
        .propagate_natural_height(true)
        .max_content_height(560)
        .child(&content)
        .build();
    card.append(&scroll);
    let close = gtk::Button::with_label("Close");
    close.set_halign(gtk::Align::End);
    let d = dialog.clone();
    close.connect_clicked(move |_| d.close());
    card.append(&close);
    dialog.present();
}

pub fn overlay() -> Option<gtk::Overlay> {
    ui().map(|u| u.borrow().overlay.clone())
}

// ---------- Sidebar ----------

fn compact() -> bool {
    narrow() || prefs::get().sidebar_collapsed || ROOM.with(|r| r.get())
}

/// The sidebar shows only icons when the window is narrow or the user collapsed it.
fn refresh_compact() {
    let Some(ui) = ui() else { return };
    let nav = ui.borrow().nav.clone();
    let compact = compact();
    sidebar::set_compact(&nav, compact);
    set_compact_show(nav.upcast_ref(), compact);
    refresh_sidebar();
}

/// The icon rail's own marks (`compact-show`): the bars where a group starts.
fn set_compact_show(w: &gtk::Widget, compact: bool) {
    if w.has_css_class("compact-show") {
        w.set_visible(compact);
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        set_compact_show(&c, compact);
        child = c.next_sibling();
    }
}

pub fn toggle_sidebar() {
    // Icons only to make room for the Inspector: the toggle just brings it back.
    if ROOM.with(|r| r.replace(false)) {
        refresh_compact();
        return;
    }
    prefs::update(|p| p.sidebar_collapsed = !p.sidebar_collapsed);
    refresh_compact();
}

// ---------- Keys ----------

fn install_keys(window: &gtk::ApplicationWindow) {
    // Capture phase: these work wherever focus is.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let w2 = window.clone();
    keys.connect_key_pressed(move |_, key, _, mods| {
        let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
        let stop = glib::Propagation::Stop;
        if settings_dialog::is_open() {
            return match key {
                gdk::Key::Escape => {
                    settings_dialog::escape();
                    stop
                }
                gdk::Key::f if ctrl => {
                    settings_dialog::focus_search();
                    stop
                }
                gdk::Key::q if ctrl => {
                    w2.close();
                    stop
                }
                _ => glib::Propagation::Proceed,
            };
        }
        // A line of text being typed (a cell, the find field) keeps its own undo.
        let in_entry = gtk::prelude::GtkWindowExt::focus(&w2).is_some_and(|f| f.is::<gtk::Text>());
        let in_doc = app::front().is_some_and(doc::is_open_in);
        let k = key.to_lower();
        if ctrl && matches!(key, gdk::Key::Tab | gdk::Key::ISO_Left_Tab | gdk::Key::KP_Tab) {
            cycle_tabs(!shift && key != gdk::Key::ISO_Left_Tab);
            return stop;
        }
        if ctrl && mods.contains(gdk::ModifierType::ALT_MASK) && in_doc {
            match k {
                gdk::Key::equal | gdk::Key::plus | gdk::Key::KP_Add => doc::zoom_step(true),
                gdk::Key::minus | gdk::Key::KP_Subtract => doc::zoom_step(false),
                gdk::Key::_0 | gdk::Key::KP_0 => doc::set_zoom(1.0),
                gdk::Key::i => toggle_inspector(),
                _ => return glib::Propagation::Proceed,
            }
            return stop;
        }
        if ctrl {
            let handled = match k {
                gdk::Key::f if shift && in_doc => {
                    toggle_focus();
                    true
                }
                gdk::Key::f => {
                    find_or_search(false);
                    true
                }
                gdk::Key::h if in_doc => {
                    find_or_search(true);
                    true
                }
                gdk::Key::b if shift => {
                    toggle_sidebar();
                    true
                }
                gdk::Key::b if !in_doc => {
                    toggle_sidebar();
                    true
                }
                gdk::Key::n if in_doc => {
                    doc::new_document(None);
                    true
                }
                gdk::Key::n => {
                    if let Some(b) = ui().map(|u| u.borrow().new_button.clone()) {
                        nyx_ui::menu::popup_below(&b, new_entries());
                    }
                    true
                }
                gdk::Key::o => {
                    doc::open_dialog();
                    true
                }
                gdk::Key::s if shift && in_doc => {
                    doc::save_as();
                    true
                }
                gdk::Key::s if in_doc => {
                    doc::save();
                    true
                }
                gdk::Key::e if in_doc => {
                    doc::export_pdf_dialog();
                    true
                }
                gdk::Key::p if in_doc => {
                    doc::print_now();
                    true
                }
                gdk::Key::comma => {
                    settings_dialog::open();
                    true
                }
                gdk::Key::z if in_doc && !in_entry && shift => {
                    if let Some(e) = doc::editor() {
                        e.redo();
                    }
                    true
                }
                gdk::Key::z if in_doc && !in_entry => {
                    if let Some(e) = doc::editor() {
                        e.undo();
                    }
                    true
                }
                gdk::Key::y if in_doc && !in_entry => {
                    if let Some(e) = doc::editor() {
                        e.redo();
                    }
                    true
                }
                gdk::Key::w if in_doc => {
                    if let Some(t) = app::front() {
                        doc::close(t);
                    }
                    true
                }
                gdk::Key::q | gdk::Key::w => {
                    w2.close();
                    true
                }
                _ => false,
            };
            if handled {
                return stop;
            }
        }
        match key {
            gdk::Key::F1 if ctrl => {
                toggle_inspector();
                return stop;
            }
            gdk::Key::F1 => {
                show_shortcuts();
                return stop;
            }
            gdk::Key::F11 => {
                toggle_fullscreen();
                return stop;
            }
            gdk::Key::Escape => {
                let search = ui().map(|u| u.borrow().search.clone());
                if let Some(tab) = app::front()
                    && find_of(tab).is_some_and(|f| f.bar.is_visible())
                {
                    hide_find(tab);
                    return stop;
                }
                if in_doc && inspector().is_some_and(|i| i.escape()) {
                    return stop;
                }
                if let Some(s) = search.filter(|s| !s.text().is_empty()) {
                    s.set_text("");
                    return stop;
                }
                if w2.is_fullscreen() && !in_doc {
                    set_fullscreen(false);
                    return stop;
                }
            }
            _ => {}
        }
        if in_doc
            && let Some(e) = doc::editor()
            && e.key(key, mods)
        {
            return stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
}

pub fn toggle_fullscreen() {
    let on = window().is_some_and(|w| w.is_fullscreen());
    set_fullscreen(!on);
}

pub fn set_fullscreen(on: bool) {
    let Some(w) = window() else { return };
    if on {
        w.fullscreen();
    } else {
        w.unfullscreen();
    }
    let Some(ui) = ui() else { return };
    ui.borrow_mut().fullscreen = on;
    let u = ui.borrow();
    u.refresh_nav();
    for c in &u.chrome {
        c.set_visible(!on && !(FOCUS.with(|f| f.get()) && c == u.tabs_bar.upcast_ref::<gtk::Widget>()));
    }
}

fn set_narrow(narrow: bool) {
    NARROW.with(|n| n.set(narrow));
    crate::write::fit_comment_panel();
    let Some(ui) = ui() else { return };
    {
        let u = ui.borrow();
        u.search.set_max_width_chars(if narrow { 14 } else { 40 });
        u.search_hint.set_visible(!narrow && u.search.text().is_empty());
        let mut page = u.stack.first_child();
        while let Some(p) = page {
            mark_narrow(&p, narrow);
            page = p.next_sibling();
        }
    }
    home::set_narrow(narrow);
    refresh_header();
    refresh_status();
}

/// A page's body takes `.narrow` in a narrow window.
fn mark_narrow(page: &gtk::Widget, narrow: bool) {
    let body = page
        .downcast_ref::<gtk::ScrolledWindow>()
        .and_then(|s| s.child())
        .and_then(|v| v.first_child())
        .unwrap_or_else(|| page.clone());
    if narrow {
        body.add_css_class("narrow");
    } else {
        body.remove_css_class("narrow");
    }
}

fn close_narrow_panels() {
    for i in inspectors() {
        i.set_open(false);
    }
    crate::sheets::close_chart_pane();
}

pub fn narrow() -> bool {
    NARROW.with(|n| n.get())
}

/// Show a page: `home`, `recent`, `templates`, `settings`, a tab's (`tab-3`), a kind's
/// id (its front file), or `document` (the file being worked on).
pub fn navigate(id: &str) {
    if id == "settings" {
        settings_dialog::open();
        return;
    }
    if id == "document" {
        match app::tab().filter(|t| doc::is_open_in(*t)) {
            Some(t) => show_document(t),
            None => show_kind(app::kind()),
        }
        return;
    }
    if let Some(k) = Kind::parse(id) {
        show_kind(k);
        return;
    }
    show_page(id);
}

fn show_page(id: &str) {
    let Some(ui) = ui() else { return };
    let tab = Tab::id_of_page(id).and_then(|n| doc::tabs().into_iter().find(|t| t.id == n));
    let id = if tab.is_none() && !PAGES.contains(&id) { "home" } else { id };
    let was = app::front();
    app::set_front(tab);
    if let Some(t) = tab {
        doc::seen(t);
    }
    {
        let mut u = ui.borrow_mut();
        u.stack.set_visible_child_name(id);
        u.current = id.to_string();
        u.refresh_nav();
    }
    // The sidebar goes to icons for an Inspector floating over this file, and back for one that isn't.
    let room = tab.is_some() && inspector().is_some_and(|i| i.is_open() && i.is_floating());
    if room || ROOM.with(|r| r.get()) {
        make_room(room);
    }
    if tab.is_none() {
        home::refresh();
    }
    if was != tab || tab.is_none() {
        refresh_all();
    }
}

/// Show a short message at the bottom of the window. It replaces any toast showing.
pub fn toast(message: &str) {
    let Some(overlay) = overlay() else {
        eprintln!("nyx-office: {message}");
        return;
    };
    if let Some(old) = TOAST.with(|t| t.borrow_mut().take()) {
        overlay.remove_overlay(&old);
    }
    let label = gtk::Label::new(Some(message));
    label.set_wrap(true);
    label.set_max_width_chars(70);
    let bx = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bx.add_css_class("toast");
    bx.append(&label);
    bx.set_halign(gtk::Align::Center);
    bx.set_valign(gtk::Align::End);
    overlay.add_overlay(&bx);
    let widget: gtk::Widget = bx.upcast();
    TOAST.with(|t| *t.borrow_mut() = Some(widget.clone()));
    glib::timeout_add_local_once(std::time::Duration::from_millis(3500), move || {
        let mine = TOAST.with(|t| t.borrow().as_ref() == Some(&widget));
        if mine {
            TOAST.with(|t| t.borrow_mut().take());
            overlay.remove_overlay(&widget);
        }
    });
}

/// The page showing: `home`, `recent`, `templates` or a tab's (`tab-3`).
pub fn current() -> String {
    ui().map(|u| u.borrow().current.clone()).unwrap_or_default()
}

pub fn window() -> Option<gtk::ApplicationWindow> {
    ui().map(|u| u.borrow().window.clone())
}
