//! Nyx-style building blocks: pages, groups and option rows.

pub use nyx_ui::widgets::*;

use gtk::prelude::*;

// ---------- Page / group ----------

pub struct Page {
    pub root: gtk::ScrolledWindow,
    pub body: gtk::Box,
}

/// A page. Its name shows in the top bar (or the settings dialog's header), so
/// the page itself starts straight with its content.
pub fn page(section: &str) -> Page {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.add_css_class("settings-page");
    body.add_css_class(&format!("page-{section}"));

    body.set_hexpand(true);

    let root = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        // Scrolls with wheel, trackpad and keyboard; no visible scrollbar.
        .vscrollbar_policy(gtk::PolicyType::External)
        .child(&body)
        .vexpand(true)
        .build();
    Page { root, body }
}

impl Page {
    pub fn group(&self, title: &str) -> Group {
        let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
        wrapper.add_css_class("settings-group");
        // The settings dialog lists groups by this name.
        wrapper.set_widget_name(title);
        if !title.is_empty() {
            let l = gtk::Label::new(Some(&title.to_uppercase()));
            l.add_css_class("group-title");
            l.set_xalign(0.0);
            wrapper.append(&l);
        }
        // Rows join into one card, split by hairlines.
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.add_css_class("group-list");
        wrapper.append(&list);
        self.body.append(&wrapper);
        Group { wrapper, list }
    }
}

#[derive(Clone)]
pub struct Group {
    pub wrapper: gtk::Box,
    pub list: gtk::Box,
}

impl Group {
    pub fn add(&self, w: &impl IsA<gtk::Widget>) {
        self.list.append(w);
    }

    pub fn note(&self, text: &str) {
        let l = gtk::Label::new(None);
        l.set_markup(text);
        l.add_css_class("group-note");
        l.set_xalign(0.0);
        l.set_wrap(true);
        // Notes sit just under the group title.
        self.wrapper.insert_child_after(&l, self.wrapper.first_child().as_ref());
    }
}

// ---------- Config files and bar buttons ----------

// ---------- Rows ----------

/// A box that shows its one child larger or smaller: the child is laid out at
/// 1/zoom of the room and drawn scaled up by zoom (clicks are mapped back by
/// GTK), so nothing inside needs to know.
pub fn zoom_box(child: &impl IsA<gtk::Widget>) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.set_layout_manager(Some(zoom::ZoomLayout::new()));
    b.append(child);
    b
}

/// Set the zoom of a `zoom_box` (1.0 is actual size).
pub fn set_zoom(b: &gtk::Box, z: f64) {
    if let Some(l) = b.layout_manager().and_downcast::<zoom::ZoomLayout>() {
        l.set_zoom(z);
        b.queue_resize();
    }
}

/// A box that lays its children out at their own width in rows, starting a new row
/// when the next one doesn't fit (chips, tags).
pub fn wrap_box(spacing: i32, row_spacing: i32) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.set_layout_manager(Some(wrap::WrapLayout::new(spacing, row_spacing)));
    b
}

mod wrap {
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use std::cell::Cell;

    mod imp {
        use super::*;

        #[derive(Default)]
        pub struct WrapLayout {
            pub spacing: Cell<i32>,
            pub row_spacing: Cell<i32>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for WrapLayout {
            const NAME: &'static str = "NxoWrapLayout";
            type Type = super::WrapLayout;
            type ParentType = gtk::LayoutManager;
        }

        impl ObjectImpl for WrapLayout {}

        fn children(w: &gtk::Widget) -> Vec<gtk::Widget> {
            let mut out = Vec::new();
            let mut child = w.first_child();
            while let Some(c) = child {
                if c.should_layout() {
                    out.push(c.clone());
                }
                child = c.next_sibling();
            }
            out
        }

        impl WrapLayout {
            /// Each child's place (x, y, width, height) in `width`, and the total height.
            fn place(&self, kids: &[gtk::Widget], width: i32) -> (Vec<(i32, i32, i32, i32)>, i32) {
                let (gap, row_gap) = (self.spacing.get(), self.row_spacing.get());
                let mut out = Vec::with_capacity(kids.len());
                let (mut x, mut y, mut row_h) = (0, 0, 0);
                for c in kids {
                    let w = c.measure(gtk::Orientation::Horizontal, -1).1.min(width.max(1));
                    let h = c.measure(gtk::Orientation::Vertical, w).1;
                    if x > 0 && x + w > width {
                        x = 0;
                        y += row_h + row_gap;
                        row_h = 0;
                    }
                    out.push((x, y, w, h));
                    x += w + gap;
                    row_h = row_h.max(h);
                }
                (out, y + row_h)
            }
        }

        impl LayoutManagerImpl for WrapLayout {
            fn request_mode(&self, _widget: &gtk::Widget) -> gtk::SizeRequestMode {
                gtk::SizeRequestMode::HeightForWidth
            }

            fn measure(&self, widget: &gtk::Widget, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
                let kids = children(widget);
                if orientation == gtk::Orientation::Horizontal {
                    let min = kids.iter().map(|c| c.measure(orientation, -1).0).max().unwrap_or(0);
                    let gaps = self.spacing.get() * (kids.len() as i32 - 1).max(0);
                    let nat = kids.iter().map(|c| c.measure(orientation, -1).1).sum::<i32>() + gaps;
                    return (min, nat, -1, -1);
                }
                let width = if for_size < 0 { i32::MAX / 2 } else { for_size };
                let h = self.place(&kids, width).1;
                (h, h, -1, -1)
            }

            fn allocate(&self, widget: &gtk::Widget, width: i32, _height: i32, _baseline: i32) {
                let kids = children(widget);
                let (places, _) = self.place(&kids, width);
                for (c, (x, y, w, h)) in kids.iter().zip(places) {
                    c.size_allocate(&gtk::Allocation::new(x, y, w, h), -1);
                }
            }
        }
    }

    glib::wrapper! {
        pub struct WrapLayout(ObjectSubclass<imp::WrapLayout>) @extends gtk::LayoutManager;
    }

    impl WrapLayout {
        pub fn new(spacing: i32, row_spacing: i32) -> WrapLayout {
            let l: WrapLayout = glib::Object::new();
            l.imp().spacing.set(spacing);
            l.imp().row_spacing.set(row_spacing);
            l
        }
    }
}

mod zoom {
    use gtk::glib;
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use std::cell::Cell;

    mod imp {
        use super::*;

        pub struct ZoomLayout {
            pub zoom: Cell<f64>,
        }

        impl Default for ZoomLayout {
            fn default() -> Self {
                ZoomLayout { zoom: Cell::new(1.0) }
            }
        }

        #[glib::object_subclass]
        impl ObjectSubclass for ZoomLayout {
            const NAME: &'static str = "NxoZoomLayout";
            type Type = super::ZoomLayout;
            type ParentType = gtk::LayoutManager;
        }

        impl ObjectImpl for ZoomLayout {}

        fn child(w: &gtk::Widget) -> Option<gtk::Widget> {
            w.first_child().filter(|c| c.should_layout())
        }

        impl LayoutManagerImpl for ZoomLayout {
            fn request_mode(&self, widget: &gtk::Widget) -> gtk::SizeRequestMode {
                child(widget).map_or(gtk::SizeRequestMode::ConstantSize, |c| c.request_mode())
            }

            fn measure(&self, widget: &gtk::Widget, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
                let Some(c) = child(widget) else { return (0, 0, -1, -1) };
                let z = self.zoom.get();
                if orientation == gtk::Orientation::Horizontal {
                    // Asks for little width: a narrow window shrinks the child to fit instead.
                    let (min, nat, _, _) = c.measure(orientation, -1);
                    return ((min as f64 * z * FIT_FLOOR).ceil() as i32, (nat as f64 * z).ceil() as i32, -1, -1);
                }
                let z = if for_size < 0 { z } else { self.fitted(&c, for_size) };
                let for_child = if for_size < 0 { -1 } else { (for_size as f64 / z).floor() as i32 };
                let (min, nat, _, _) = c.measure(orientation, for_child);
                ((min as f64 * z).ceil() as i32, (nat as f64 * z).ceil() as i32, -1, -1)
            }

            fn allocate(&self, widget: &gtk::Widget, width: i32, height: i32, _baseline: i32) {
                let Some(c) = child(widget) else { return };
                let z = self.fitted(&c, width);
                let w = ((width as f64 / z).floor() as i32).max(c.measure(gtk::Orientation::Horizontal, -1).0);
                let h = ((height as f64 / z).floor() as i32).max(c.measure(gtk::Orientation::Vertical, w).0);
                let t = gtk::gsk::Transform::new().scale(z as f32, z as f32);
                c.allocate(w, h, -1, Some(t));
            }
        }

        /// The smallest share of its own minimum width the child is shrunk to.
        const FIT_FLOOR: f64 = 0.35;
        /// Room up to this much wider than the child fills with it (a half-screen
        /// window), rather than leaving a thin margin on each side.
        const FILL_UNDER: f64 = 1.12;

        impl ZoomLayout {
            /// The zoom for `width`: the chosen one; less when the child wouldn't fit
            /// at it; a little more when it nearly fills the room anyway.
            fn fitted(&self, c: &gtk::Widget, width: i32) -> f64 {
                let z = self.zoom.get();
                let min = c.measure(gtk::Orientation::Horizontal, -1).0;
                if width <= 0 || min <= 0 {
                    return z;
                }
                let fit = width as f64 / min as f64;
                if fit < z * FILL_UNDER { fit.max(z * FIT_FLOOR) } else { z }
            }
        }
    }

    glib::wrapper! {
        pub struct ZoomLayout(ObjectSubclass<imp::ZoomLayout>) @extends gtk::LayoutManager;
    }

    impl ZoomLayout {
        pub fn new() -> ZoomLayout {
            glib::Object::new()
        }

        pub fn set_zoom(&self, z: f64) {
            self.imp().zoom.set(z.clamp(0.25, 4.0));
        }
    }
}
