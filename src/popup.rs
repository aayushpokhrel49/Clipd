use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf};

use crate::store::{self, ClipItem};

pub const WIDTH: i32 = 440;
pub const HEIGHT: i32 = 470;

/// Thumbnails are capped so a huge history cannot balloon memory.
const MAX_THUMBS: usize = 200;
/// Only this many matching rows are built; refine the search to go deeper.
const MAX_ROWS: usize = 400;

const CSS: &str = r#"
.clipd-window { background-color: transparent; }
.clipd-frame {
  background-color: @theme_bg_color;
  border: 1px solid alpha(@theme_fg_color, 0.18);
  border-radius: 12px;
}
.clipd-header { padding: 6px 8px 0 14px; }
.clipd-header .clipd-title { font-weight: bold; opacity: 0.85; }
.clipd-titlebutton {
  min-width: 24px;
  min-height: 24px;
  padding: 0;
  border: none;
  border-radius: 12px;
  background-color: transparent;
  background-image: none;
  box-shadow: none;
  font-size: 110%;
}
.clipd-titlebutton:hover { background-color: alpha(@theme_fg_color, 0.12); }
.clipd-close:hover { background-color: #e01b24; color: #ffffff; }
.clipd-frame list { background-color: transparent; }
.clipd-frame list row { border-radius: 8px; margin: 1px 5px; padding: 1px; }
.clipd-frame list row:selected {
  background-color: @theme_selected_bg_color;
  color: @theme_selected_fg_color;
}
.clipd-frame .dim-label { opacity: 0.62; }
.clipd-footer { padding: 2px 12px 8px 12px; }
.clipd-footer label { font-size: 90%; }
"#;

fn install_css() {
    let provider = gtk::CssProvider::new();
    if provider.load_from_data(CSS.as_bytes()).is_err() {
        return;
    }
    if let Some(screen) = gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// Pointer root coords plus window origin when a header drag started.
type DragStart = (f64, f64, i32, i32);

/// Per-item display data, computed once and reused across keystrokes.
struct RowMeta {
    lower: String,
    preview: String,
    chars: usize,
}

impl RowMeta {
    fn new(item: &ClipItem) -> Self {
        let lower = item.search_blob().to_lowercase();
        if item.is_image() {
            RowMeta {
                lower,
                preview: String::new(),
                chars: 0,
            }
        } else {
            let text = item.text.as_deref().unwrap_or_default();
            let preview: String = text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(300)
                .collect();
            RowMeta {
                lower,
                preview,
                chars: text.chars().count(),
            }
        }
    }
}

pub struct Popup {
    pub window: gtk::Window,
    search: gtk::SearchEntry,
    list: gtk::ListBox,
    status: gtk::Label,
    history: Rc<RefCell<Vec<ClipItem>>>,
    on_activate: Rc<dyn Fn(u64)>,
    filter: Rc<RefCell<String>>,
    ids: Rc<RefCell<Vec<u64>>>,
    images_dir: PathBuf,
    thumb_cache: RefCell<HashMap<String, gdk_pixbuf::Pixbuf>>,
    row_cache: RefCell<HashMap<u64, RowMeta>>,
}

impl Popup {
    pub fn new(
        app: &gtk::Application,
        history: Rc<RefCell<Vec<ClipItem>>>,
        on_activate: Rc<dyn Fn(u64)>,
        on_delete: Rc<dyn Fn(u64)>,
    ) -> Rc<Popup> {
        install_css();

        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Clipboard");
        window.set_type_hint(gdk::WindowTypeHint::PopupMenu);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);
        window.set_decorated(false);
        window.set_keep_above(true);
        window.set_resizable(false);
        window.set_default_size(WIDTH, HEIGHT);
        window.set_accept_focus(true);
        window.style_context().add_class("clipd-window");
        if let Some(screen) = gdk::Screen::default() {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
            }
        }
        window.set_app_paintable(true);

        let minimize = gtk::Button::with_label("\u{2013}");
        minimize.set_relief(gtk::ReliefStyle::None);
        minimize.set_tooltip_text(Some("Minimize"));
        minimize.style_context().add_class("clipd-titlebutton");

        let close = gtk::Button::with_label("\u{00d7}");
        close.set_relief(gtk::ReliefStyle::None);
        close.set_tooltip_text(Some("Close"));
        close.style_context().add_class("clipd-titlebutton");
        close.style_context().add_class("clipd-close");

        let title = gtk::Label::new(Some("Clipboard"));
        title.set_xalign(0.0);
        title.style_context().add_class("clipd-title");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        header.style_context().add_class("clipd-header");
        header.add_events(
            gdk::EventMask::BUTTON_PRESS_MASK
                | gdk::EventMask::BUTTON_RELEASE_MASK
                | gdk::EventMask::BUTTON1_MOTION_MASK,
        );
        header.pack_start(&title, true, true, 0);
        header.pack_end(&close, false, false, 0);
        header.pack_end(&minimize, false, false, 0);

        header.connect_realize(|h| {
            if let Some(win) = h.window() {
                if let Some(display) = gdk::Display::default() {
                    if let Some(cursor) = gdk::Cursor::from_name(&display, "move") {
                        win.set_cursor(Some(&cursor));
                    }
                }
            }
        });

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Search clipboard..."));
        search.set_margin_top(10);
        search.set_margin_bottom(6);
        search.set_margin_start(10);
        search.set_margin_end(10);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(true);

        let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_shadow_type(gtk::ShadowType::None);
        scroll.set_min_content_height(HEIGHT - 136);
        scroll.add(&list);

        let status = gtk::Label::new(None);
        status.set_xalign(0.0);
        status.set_ellipsize(gtk::pango::EllipsizeMode::End);
        status.style_context().add_class("dim-label");

        let hint = gtk::Label::new(Some(
            "\u{2191}\u{2193} move  \u{2022}  \u{21b5} paste  \u{2022}  Del remove  \u{2022}  Esc close",
        ));
        hint.set_xalign(1.0);
        hint.style_context().add_class("dim-label");

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        footer.style_context().add_class("clipd-footer");
        footer.pack_start(&status, true, true, 0);
        footer.pack_end(&hint, false, false, 0);

        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.style_context().add_class("clipd-frame");
        frame.pack_start(&header, false, false, 0);
        frame.pack_start(&search, false, false, 0);
        frame.pack_start(&scroll, true, true, 0);
        frame.pack_start(&footer, false, false, 0);
        window.add(&frame);
        app.add_window(&window);

        Rc::new_cyclic(move |weak: &Weak<Popup>| {
            let filter = Rc::new(RefCell::new(String::new()));
            let ids = Rc::new(RefCell::new(Vec::new()));

            let drag: Rc<Cell<Option<DragStart>>> = Rc::new(Cell::new(None));

            {
                let window = window.clone();
                minimize.connect_clicked(move |_| window.iconify());
            }

            {
                let window = window.clone();
                close.connect_clicked(move |_| window.hide());
            }

            {
                let window = window.clone();
                let drag = drag.clone();
                header.connect_button_press_event(move |_, ev| {
                    if ev.button() != 1 {
                        return glib::Propagation::Proceed;
                    }
                    let (rx, ry) = ev.root();
                    let (wx, wy) = window.position();
                    drag.set(Some((rx, ry, wx, wy)));
                    glib::Propagation::Stop
                });
            }

            {
                let window = window.clone();
                let drag = drag.clone();
                header.connect_motion_notify_event(move |_, ev| {
                    let Some((sx, sy, wx, wy)) = drag.get() else {
                        return glib::Propagation::Proceed;
                    };
                    let (rx, ry) = ev.root();
                    let nx = wx + (rx - sx).round() as i32;
                    let ny = wy + (ry - sy).round() as i32;
                    window.move_(nx, ny);
                    glib::Propagation::Stop
                });
            }

            {
                let window = window.clone();
                let drag = drag.clone();
                header.connect_button_release_event(move |_, _| {
                    if drag.take().is_some() {
                        let (x, y) = window.position();
                        store::save_pos(x, y);
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                });
            }

            {
                let filter = filter.clone();
                let weak = weak.clone();
                search.connect_search_changed(move |s| {
                    *filter.borrow_mut() = s.text().to_string();
                    if let Some(p) = weak.upgrade() {
                        p.refresh();
                    }
                });
            }

            {
                let weak = weak.clone();
                search.connect_activate(move |_| {
                    if let Some(p) = weak.upgrade() {
                        p.activate_selected();
                    }
                });
            }

            {
                let weak = weak.clone();
                search.connect_stop_search(move |_| {
                    if let Some(p) = weak.upgrade() {
                        p.window.hide();
                    }
                });
            }

            {
                let list = list.clone();
                search.connect_key_press_event(move |_, ev| match ev.keyval() {
                    gdk::keys::constants::Down | gdk::keys::constants::Up => {
                        list.grab_focus();
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                });
            }

            {
                let on_activate = on_activate.clone();
                let ids = ids.clone();
                let window = window.clone();
                list.connect_row_activated(move |_, row| {
                    let idx = row.index();
                    if idx < 0 {
                        return;
                    }
                    let id = ids.borrow().get(idx as usize).copied();
                    if let Some(id) = id {
                        window.hide();
                        on_activate(id);
                    }
                });
            }

            {
                let on_delete = on_delete.clone();
                let ids = ids.clone();
                let list_for_signal = list.clone();
                let weak = weak.clone();
                let search = search.clone();
                list.connect_key_press_event(move |_, ev| match ev.keyval() {
                    gdk::keys::constants::Delete => {
                        if let Some(row) = list_for_signal.selected_row() {
                            let idx = row.index();
                            let id = ids.borrow().get(idx as usize).copied();
                            if let Some(id) = id {
                                on_delete(id);
                                if let Some(p) = weak.upgrade() {
                                    p.refresh();
                                }
                            }
                        }
                        glib::Propagation::Stop
                    }
                    gdk::keys::constants::Escape => {
                        if let Some(p) = weak.upgrade() {
                            p.window.hide();
                        }
                        glib::Propagation::Stop
                    }
                    gdk::keys::constants::BackSpace => {
                        let mut text = search.text().to_string();
                        text.pop();
                        search.grab_focus();
                        search.set_text(&text);
                        search.set_position(-1);
                        glib::Propagation::Stop
                    }
                    _ => {
                        if let Some(ch) = ev.keyval().to_unicode() {
                            if !ch.is_control() {
                                let mut text = search.text().to_string();
                                text.push(ch);
                                search.grab_focus();
                                search.set_text(&text);
                                search.set_position(-1);
                                return glib::Propagation::Stop;
                            }
                        }
                        glib::Propagation::Proceed
                    }
                });
            }

            {
                let window2 = window.clone();
                window.connect_key_press_event(move |_, ev| match ev.keyval() {
                    gdk::keys::constants::Escape => {
                        window2.hide();
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                });
            }

            {
                let window2 = window.clone();
                window.connect_focus_out_event(move |_, _| {
                    let window2 = window2.clone();
                    glib::timeout_add_local_once(Duration::from_millis(140), move || {
                        if window2.is_visible() && !window2.is_active() {
                            window2.hide();
                        }
                    });
                    glib::Propagation::Proceed
                });
            }

            Popup {
                window,
                search,
                list,
                status,
                history,
                on_activate,
                filter,
                ids,
                images_dir: store::images_dir(),
                thumb_cache: RefCell::new(HashMap::new()),
                row_cache: RefCell::new(HashMap::new()),
            }
        })
    }

    pub fn popup_at(&self, x: i32, y: i32) {
        self.filter.borrow_mut().clear();
        self.search.set_text("");
        self.refresh();

        let (mut px, mut py) = store::load_pos().unwrap_or((x, y));
        if let Some(display) = gdk::Display::default() {
            if let Some(mon) = display.monitor_at_point(px, py) {
                let wa = mon.workarea();
                px = px.max(wa.x()).min(wa.x() + wa.width() - WIDTH);
                py = py.max(wa.y()).min(wa.y() + wa.height() - HEIGHT);
            }
        }

        self.window.deiconify();
        self.window.move_(px, py);
        self.window.show_all();
        self.window.present();
        self.search.grab_focus();
        if let Some(first) = self.list.row_at_index(0) {
            self.list.select_row(Some(&first));
        }
    }

    pub fn refresh(&self) {
        for child in self.list.children() {
            self.list.remove(&child);
        }

        let filter = self.filter.borrow().to_lowercase();
        let history = self.history.borrow();

        // Drop cached metadata for items that are no longer in the history.
        {
            let mut present_ids = HashSet::with_capacity(history.len());
            let mut present_imgs = HashSet::new();
            for item in history.iter() {
                present_ids.insert(item.id);
                if let Some(img) = &item.image {
                    present_imgs.insert(img.as_str());
                }
            }
            self.row_cache
                .borrow_mut()
                .retain(|id, _| present_ids.contains(id));
            self.thumb_cache
                .borrow_mut()
                .retain(|name, _| present_imgs.contains(name.as_str()));
        }

        {
            let mut cache = self.row_cache.borrow_mut();
            for item in history.iter() {
                cache.entry(item.id).or_insert_with(|| RowMeta::new(item));
            }
        }

        let mut ids = self.ids.borrow_mut();
        ids.clear();

        let cache = self.row_cache.borrow();
        let mut matched = 0usize;
        for item in history.iter() {
            let Some(meta) = cache.get(&item.id) else {
                continue;
            };
            if !filter.is_empty() && !meta.lower.contains(&filter) {
                continue;
            }
            matched += 1;
            if ids.len() >= MAX_ROWS {
                continue;
            }
            self.list.add(&self.build_row(item, meta));
            ids.push(item.id);
        }
        drop(cache);

        let total = history.len();
        let shown = ids.len();
        let status = if shown == 0 {
            if filter.is_empty() {
                "No clipboard history yet"
            } else {
                "No matches"
            }
            .to_string()
        } else if filter.is_empty() {
            if shown < total {
                format!("{shown} of {total} items")
            } else {
                format!("{total} items")
            }
        } else if matched > shown {
            format!("{matched} matches \u{2022} showing {shown}")
        } else {
            format!("{matched} matches")
        };
        self.status.set_text(&status);

        if shown == 0 {
            let row = gtk::ListBoxRow::new();
            row.set_selectable(false);
            row.set_activatable(false);
            let label = gtk::Label::new(Some(&status));
            label.style_context().add_class("dim-label");
            label.set_margin_top(28);
            label.set_margin_bottom(28);
            row.add(&label);
            self.list.add(&row);
        }

        self.list.show_all();
        if shown > 0 {
            if let Some(first) = self.list.row_at_index(0) {
                self.list.select_row(Some(&first));
            }
        }
    }

    fn activate_selected(&self) {
        let row = self
            .list
            .selected_row()
            .or_else(|| self.list.row_at_index(0));
        if let Some(row) = row {
            let idx = row.index();
            if idx < 0 {
                return;
            }
            let id = self.ids.borrow().get(idx as usize).copied();
            if let Some(id) = id {
                self.window.hide();
                (self.on_activate)(id);
            }
        }
    }

    fn build_row(&self, item: &ClipItem, meta: &RowMeta) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        hbox.set_border_width(6);
        hbox.set_margin_start(6);
        hbox.set_margin_end(6);

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);

        if item.is_image() {
            let image = self.thumbnail(item.image.as_deref().unwrap_or_default());
            hbox.pack_start(&image, false, false, 0);

            let title = gtk::Label::new(Some("Image"));
            title.set_xalign(0.0);
            vbox.pack_start(&title, false, false, 0);

            let meta_label =
                gtk::Label::new(Some(&format!("copied {}", store::human_age(item.created))));
            meta_label.set_xalign(0.0);
            meta_label.style_context().add_class("dim-label");
            vbox.pack_start(&meta_label, false, false, 0);
        } else {
            let title = gtk::Label::new(Some(&meta.preview));
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title.set_single_line_mode(true);
            title.set_line_wrap(false);
            title.set_max_width_chars(48);
            vbox.pack_start(&title, false, false, 0);

            let meta_label = gtk::Label::new(Some(&format!(
                "{} chars  \u{2022}  copied {}",
                meta.chars,
                store::human_age(item.created)
            )));
            meta_label.set_xalign(0.0);
            meta_label.style_context().add_class("dim-label");
            vbox.pack_start(&meta_label, false, false, 0);
        }

        hbox.pack_start(&vbox, true, true, 0);
        row.add(&hbox);
        row
    }

    /// Return a scaled thumbnail, decoding it from disk only once per file.
    fn thumbnail(&self, name: &str) -> gtk::Image {
        if let Some(pb) = self.thumb_cache.borrow().get(name).cloned() {
            return gtk::Image::from_pixbuf(Some(&pb));
        }
        match gdk_pixbuf::Pixbuf::from_file_at_scale(self.images_dir.join(name), 150, 84, true) {
            Ok(pb) => {
                let image = gtk::Image::from_pixbuf(Some(&pb));
                let mut cache = self.thumb_cache.borrow_mut();
                if cache.len() >= MAX_THUMBS {
                    cache.clear();
                }
                cache.insert(name.to_string(), pb);
                image
            }
            Err(_) => {
                gtk::Image::from_icon_name(Some("image-missing"), gtk::IconSize::LargeToolbar)
            }
        }
    }
}