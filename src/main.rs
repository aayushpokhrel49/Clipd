mod clipboard;
mod popup;
mod store;
mod update;
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

    let (quit_tx, quit_rx) = glib::MainContext::channel::<()>(glib::Priority::DEFAULT);
    {
        let app = app.clone();
        quit_rx.attach(None, move |_| {
            app.quit();
            glib::ControlFlow::Break
        });
    }

    let check_updates: Rc<dyn Fn(bool)> = {
        let quit_tx = quit_tx.clone();
        Rc::new(move |manual: bool| {
            if update::disabled() {
                if manual {
                    update::notify("clipd", "Updates are disabled (CLIPD_NO_UPDATE is set)");
                }
                return;
            }
            let quit_tx = quit_tx.clone();
            let spawned = std::thread::Builder::new()
                .name("clipd-update".into())
                .spawn(move || match run_update(manual) {
                    Ok(true) => {
                        let _ = quit_tx.send(());
                    }
                    Ok(false) => {}
                    Err(e) => {
                        if manual {
                            log::warn!("update failed: {e:#}");
                            update::notify("clipd update failed", &format!("{e:#}"));
                        } else {
                            log::debug!("update check failed: {e:#}");
                        }
                    }
                });
            if let Err(e) = spawned {
                log::warn!("cannot spawn update thread: {e}");
            }
        })
    };

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
        let check_updates = check_updates.clone();
        app.connect_command_line(move |app, cmd| {
            let args = cmd.arguments();
            let action = args.get(1).map(|s| s.to_string_lossy().to_string());
            match action.as_deref() {
                Some("start") => {}
                Some("update") => check_updates(true),
                Some("clear") => store::clear(&mut history.borrow_mut()),
                Some("show") => {
                    let (x, y) = x11::pointer().unwrap_or((200, 200));
                    popup.popup_at(x as i32, y as i32);
                }
                Some("quit") => {
                    let app = app.clone();
                    glib::idle_add_local_once(move || app.quit());
                }
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

    {
        let check_updates = check_updates.clone();
        glib::timeout_add_seconds_local_once(90, move || check_updates(false));
    }
    {
        let check_updates = check_updates.clone();
        glib::timeout_add_seconds_local(6 * 3600, move || {
            check_updates(false);
            glib::ControlFlow::Continue
        });
    }

    log::info!("clipd started (Ctrl+Alt+V to open, run `clipd` to toggle)");
}

fn run_update(manual: bool) -> anyhow::Result<bool> {
    let Some(release) = update::check()? else {
        log::debug!("clipd is up to date (v{})", update::current_version());
        if manual {
            update::notify(
                "clipd",
                &format!("Already up to date (v{})", update::current_version()),
            );
        }
        return Ok(false);
    };

    if !manual && update::is_dev_build() {
        log::info!(
            "update v{} available (auto-update skipped for dev build)",
            release.version
        );
        return Ok(false);
    }

    if update::is_package_managed() {
        log::info!(
            "update v{} available (package-managed install; update via your package manager)",
            release.version
        );
        if manual {
            update::notify(
                "clipd",
                &format!(
                    "v{} is available — update it with your package manager",
                    release.version
                ),
            );
        }
        return Ok(false);
    }

    log::info!("installing update v{}...", release.version);
    let path = update::install(&release)?;
    update::notify(
        "clipd updated",
        &format!("Restarting with v{}", release.version),
    );
    update::restart(&path);
    Ok(true)
}
