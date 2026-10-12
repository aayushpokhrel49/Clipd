use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, GrabMode, InputFocus, ModMask};
use x11rb::protocol::{Event, xfixes, xtest};
use x11rb::rust_connection::RustConnection;

pub enum XOut {
    Show { x: i16, y: i16, focus: u32 },
    ClipboardChanged,
}

const XK_V: u32 = 0x0076;
const XK_CONTROL_L: u32 = 0xffe3;
const XK_SHIFT_L: u32 = 0xffe1;

pub fn spawn(tx: glib::Sender<XOut>) {
    let res = std::thread::Builder::new()
        .name("clipd-x11".into())
        .spawn(move || loop {
            if let Err(e) = run(&tx) {
                log::warn!("x11 event loop error: {e:#}; reconnecting");
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    if let Err(e) = res {
        log::error!("cannot spawn x11 thread: {e}");
    }
}

fn run(tx: &glib::Sender<XOut>) -> anyhow::Result<()> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen_num].root;

    let _ = xtest::get_version(&conn, 2, 2)?.reply();
    let _ = xfixes::query_version(&conn, 5, 0)?.reply();

    let clip_atom = xproto::intern_atom(&conn, false, b"CLIPBOARD")?.reply()?.atom;
    let v_keycode =
        lookup_keycode(&conn, XK_V).ok_or_else(|| anyhow::anyhow!("cannot find keycode for V"))?;

    let base = ModMask::CONTROL | ModMask::M1;
    // Also grab every combination with NumLock/CapsLock/ScrollLock so the
    // hotkey keeps working regardless of which lock modifiers are active.
    let lock_bits = [ModMask::LOCK, ModMask::M2, ModMask::M3, ModMask::M5];
    let mut grabbed = 0u32;
    for combo in 0..16u8 {
        let mut mask = base;
        for (bit, lm) in lock_bits.iter().enumerate() {
            if combo & (1 << bit) != 0 {
                mask |= *lm;
            }
        }
        let result = match xproto::grab_key(
            &conn,
            false,
            root,
            mask,
            v_keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        ) {
            Ok(cookie) => cookie.check().map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        match result {
            Ok(()) => grabbed += 1,
            // A conflict simply means another app already owns Ctrl+Alt+V;
            // that must not take the whole daemon (and clipboard watching) down.
            Err(e) => log::warn!("could not grab Ctrl+Alt+V ({mask:?}): {e}"),
        }
    }
    if grabbed == 0 {
        log::warn!("Ctrl+Alt+V is already in use by another program; use `clipd toggle` instead");
    }

    xfixes::select_selection_input(
        &conn,
        root,
        clip_atom,
        xfixes::SelectionEventMask::SET_SELECTION_OWNER,
    )?
    .check()?;

    conn.flush()?;
    log::info!("listening for Ctrl+Alt+V and clipboard changes");

    loop {
        let event = conn.wait_for_event()?;
        match event {
            Event::KeyPress(e) => {
                let state = u16::from(e.state);
                let ctrl = u16::from(ModMask::CONTROL);
                let alt = u16::from(ModMask::M1);
                if e.detail == v_keycode && (state & ctrl) != 0 && (state & alt) != 0 {
                    let focus = xproto::get_input_focus(&conn)
                        .ok()
                        .and_then(|c| c.reply().ok())
                        .map(|r| r.focus)
                        .unwrap_or(0);
                    let _ = tx.send(XOut::Show {
                        x: e.root_x,
                        y: e.root_y,
                        focus,
                    });
                }
            }
            Event::XfixesSelectionNotify(e) if e.selection == clip_atom && e.owner != 0 => {
                let _ = tx.send(XOut::ClipboardChanged);
            }
            _ => {}
        }
    }
}

fn lookup_keycode(conn: &RustConnection, keysym: u32) -> Option<u8> {
    let setup = conn.setup();
    let min = setup.min_keycode;
    let count = setup.max_keycode - min + 1;
    let reply = xproto::get_keyboard_mapping(conn, min, count)
        .ok()?
        .reply()
        .ok()?;
    let per = reply.keysyms_per_keycode as usize;
    if per == 0 {
        return None;
    }
    for (i, chunk) in reply.keysyms.chunks(per).enumerate() {
        if chunk.contains(&keysym) {
            return Some(min + i as u8);
        }
    }
    None
}

pub fn pointer() -> Option<(i16, i16)> {
    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen_num].root;
    let reply = xproto::query_pointer(&conn, root).ok()?.reply().ok()?;
    Some((reply.root_x, reply.root_y))
}

pub fn focus_and_paste(window: u32) {
    if let Err(e) = do_paste(window) {
        log::warn!("auto-paste failed: {e:#}");
    }
}

fn do_paste(window: u32) -> anyhow::Result<()> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen_num].root;
    let _ = xtest::get_version(&conn, 2, 2)?.reply();

    let terminal = window != 0 && is_terminal(&conn, window);
    log::debug!("auto-paste: target window={window} terminal={terminal}");

    if window != 0 {
        // Ask the window manager to focus the target window (EWMH), then fall
        // back to a direct focus request for unmanaged/override-redirect ones.
        let _ = activate(&conn, root, window);
        conn.flush()?;
        std::thread::sleep(Duration::from_millis(50));
        if let Ok(cookie) =
            xproto::set_input_focus(&conn, InputFocus::POINTER_ROOT, window, x11rb::CURRENT_TIME)
        {
            let _ = cookie.check();
        }
        conn.flush()?;
        std::thread::sleep(Duration::from_millis(60));
    }

    let ctrl = lookup_keycode(&conn, XK_CONTROL_L)
        .ok_or_else(|| anyhow::anyhow!("no Control keycode"))?;
    let v = lookup_keycode(&conn, XK_V).ok_or_else(|| anyhow::anyhow!("no V keycode"))?;
    let shift = lookup_keycode(&conn, XK_SHIFT_L);

    // Terminal emulators paste with Ctrl+Shift+V; every other app uses Ctrl+V.
    let mut sequence: Vec<(u8, bool)> = Vec::new();
    sequence.push((ctrl, true));
    if terminal {
        if let Some(shift) = shift {
            sequence.push((shift, true));
        }
    }
    sequence.push((v, true));
    sequence.push((v, false));
    if terminal {
        if let Some(shift) = shift {
            sequence.push((shift, false));
        }
    }
    sequence.push((ctrl, false));

    for (keycode, press) in sequence {
        fake_key(&conn, root, keycode, press)?;
        std::thread::sleep(Duration::from_millis(15));
    }
    Ok(())
}

/// Substrings of terminal-emulator window classes that need Ctrl+Shift+V.
const TERMINALS: &[&str] = &[
    "terminal",
    "konsole",
    "xterm",
    "urxvt",
    "rxvt",
    "alacritty",
    "kitty",
    "terminator",
    "tilix",
    "wezterm",
    "foot",
    "ghostty",
    "stterm",
    "hyper",
    "qterminal",
    "lxterminal",
];

/// True when the window's `WM_CLASS` looks like a terminal emulator.
fn is_terminal(conn: &RustConnection, window: u32) -> bool {
    let reply = xproto::get_property(
        conn,
        false,
        window,
        xproto::AtomEnum::WM_CLASS,
        xproto::AtomEnum::STRING,
        0,
        1024,
    )
    .ok()
    .and_then(|cookie| cookie.reply().ok());
    let Some(reply) = reply else {
        return false;
    };
    class_is_terminal(&String::from_utf8_lossy(&reply.value))
}

fn class_is_terminal(class: &str) -> bool {
    let class = class.to_lowercase();
    TERMINALS.iter().any(|t| class.contains(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_class_detection() {
        assert!(class_is_terminal("gnome-terminal-server\0Gnome-terminal\0"));
        assert!(class_is_terminal("Konsole\0konsole\0"));
        assert!(class_is_terminal("Alacritty\0Alacritty\0"));
        assert!(class_is_terminal("xterm\0XTerm\0"));
        assert!(!class_is_terminal("me.aayush.Aaykra\0me.aayush.Aaykra\0"));
        assert!(!class_is_terminal("firefox\0Firefox\0"));
        assert!(!class_is_terminal("code\0Code\0"));
    }
}

/// Ask the window manager to activate `window` (EWMH `_NET_ACTIVE_WINDOW`).
fn activate(conn: &RustConnection, root: u32, window: u32) -> anyhow::Result<()> {
    let atom = xproto::intern_atom(conn, false, b"_NET_ACTIVE_WINDOW")?
        .reply()?
        .atom;
    let event =
        xproto::ClientMessageEvent::new(32, window, atom, [2u32, x11rb::CURRENT_TIME, 0, 0, 0]);
    xproto::send_event(
        conn,
        false,
        root,
        xproto::EventMask::SUBSTRUCTURE_REDIRECT | xproto::EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )?
    .check()?;
    Ok(())
}

fn fake_key(conn: &RustConnection, root: u32, keycode: u8, press: bool) -> anyhow::Result<()> {
    let type_ = if press {
        xproto::KEY_PRESS_EVENT
    } else {
        xproto::KEY_RELEASE_EVENT
    };
    xtest::fake_input(conn, type_, keycode, x11rb::CURRENT_TIME, root, 0, 0, 0)?.check()?;
    conn.flush()?;
    Ok(())
}
