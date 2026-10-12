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
    let lock_bits = [ModMask::LOCK, ModMask::M2, ModMask::M3, ModMask::M5];
    for combo in 0..16u8 {
        let mut mask = base;
        for (bit, lm) in lock_bits.iter().enumerate() {
            if combo & (1 << bit) != 0 {
                mask |= *lm;
            }
        }
        xproto::grab_key(
            &conn,
            false,
            root,
            mask,
            v_keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )?
        .check()?;
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

    let ctrl = lookup_keycode(&conn, XK_CONTROL_L)
        .ok_or_else(|| anyhow::anyhow!("no Control keycode"))?;
    let v = lookup_keycode(&conn, XK_V).ok_or_else(|| anyhow::anyhow!("no V keycode"))?;

    if window != 0 {
        xproto::set_input_focus(&conn, InputFocus::POINTER_ROOT, window, x11rb::CURRENT_TIME)?
            .check()?;
        conn.flush()?;
        std::thread::sleep(Duration::from_millis(60));
    }

    fake_key(&conn, root, ctrl, true)?;
    std::thread::sleep(Duration::from_millis(15));
    fake_key(&conn, root, v, true)?;
    std::thread::sleep(Duration::from_millis(25));
    fake_key(&conn, root, v, false)?;
    std::thread::sleep(Duration::from_millis(15));
    fake_key(&conn, root, ctrl, false)?;
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
