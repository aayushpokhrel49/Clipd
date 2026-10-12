mod clipboard;
mod popup;
mod store;
mod x11;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use gtk::gio::ApplicationFlags;
use gtk::prelude::*;
use gtk::Application;

const APP_ID: &str = "com.local.Clipd";

fn main() -> glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if let Err(e) = gtk::init() {
        eprintln!("clipd: failed to initialize GTK: {e}");
        return glib::ExitCode::FAILURE;
    }
    if let Err(e) = store::ensure_dirs() {
        log::warn!("cannot create data directories: {e}");
    }

    let app = Application::new(Some(APP_ID), ApplicationFlags::HANDLES_COMMAND_LINE);
    app.connect_startup(setup);

    app.run()
}

#[allow(deprecated)]
fn setup(app: &Application) {
    std::mem::forget(app.hold());
    let history = Rc::new(RefCell::new(store::load()));
    let clip = clipboard::get_clip();
    let self_owning = Rc::new(AtomicBool::new(false));
    let last_focus = Rc::new(Cell::new(0u32));

    let (tx, rx) = glib::MainContext::channel::<x11::XOut>(glib::Priority::DEFAULT);

    let own: Rc<dyn Fn(&store::Content)> = {
        let clip = clip.clone();
        let self_owning = self_owning.clone();
        Rc::new(move |content: &store::Content| {
            self_owning.store(true, Ordering::SeqCst);
            clipboard::set(&clip, content);
            let flag = self_owning.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
                flag.store(false, Ordering::SeqCst);
            });
        })
    };

    let on_activate: Rc<dyn Fn(u64)> = {
        let history = history.clone();
        let last_focus = last_focus.clone();
        let own = own.clone();
        Rc::new(move |id: u64| {
            let content = {
                let h = history.borrow();
                h.iter().find(|i| i.id == id).and_then(store::content_of)
            };
            let Some(content) = content else {
                return;
            };
            own(&content);
            store::bump(&mut history.borrow_mut(), id);
            let focus = last_focus.get();
            std::thread::spawn(move || x11::focus_and_paste(focus));
        })
    };

    let on_delete: Rc<dyn Fn(u64)> = {
        let history = history.clone();
        Rc::new(move |id| store::remove(&mut history.borrow_mut(), id))
    };

    let popup = popup::Popup::new(app, history.clone(), on_activate, on_delete);

    {
        let popup = popup.clone();
        let clip = clip.clone();
        let history = history.clone();
        let self_owning = self_owning.clone();
        let last_focus = last_focus.clone();
        let own = own.clone();
        rx.attach(None, move |msg| {
            match msg {
                x11::XOut::Show { x, y, focus } => {
                    log::debug!("hotkey: showing popup at {x},{y} focus={focus}");
                    last_focus.set(focus);
                    popup.popup_at(x as i32, y as i32);
                }
                x11::XOut::ClipboardChanged => {
                    if self_owning.swap(false, Ordering::SeqCst) {
                        log::debug!("clipboard change was self-inflicted; ignoring");
                        return glib::ControlFlow::Continue;
                    }
                    if let Some(content) = clipboard::read(&clip) {
                        if store::record(&mut history.borrow_mut(), &content) {
                            log::debug!("captured new clipboard content");
                        }
                        own(&content);
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let popup = popup.clone();
        let history = history.clone();
        app.connect_command_line(move |app, cmd| {
            let args = cmd.arguments();
            let action = args.get(1).map(|s| s.to_string_lossy().to_string());
            match action.as_deref() {
                Some("start") => {}
                Some("clear") => store::clear(&mut history.borrow_mut()),
                Some("show") => {
                    let (x, y) = x11::pointer().unwrap_or((200, 200));
                    popup.popup_at(x as i32, y as i32);
                }
                Some("quit") => app.quit(),
                Some("toggle") | None => {
                    if popup.window.is_visible() {
                        popup.window.hide();
                    } else {
                        let (x, y) = x11::pointer().unwrap_or((200, 200));
                        popup.popup_at(x as i32, y as i32);
                    }
                }
                _ => {}
            }
            0
        });
    }

    x11::spawn(tx);
    log::info!("clipd started (Ctrl+Alt+V to open, run `clipd` to toggle)");
}
