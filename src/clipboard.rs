use gtk::gdk;
use gtk::Clipboard;

use crate::store::Content;

pub fn get_clip() -> Clipboard {
    Clipboard::get(&gdk::SELECTION_CLIPBOARD)
}

pub fn read(clip: &Clipboard) -> Option<Content> {
    let targets = clip.wait_for_targets()?;

    let hint = gdk::Atom::intern("x-kde-passwordManagerHint");
    if targets.iter().any(|a| a == &hint) {
        log::debug!("skipping password-manager clipboard entry");
        return None;
    }

    let image_atoms: Vec<gdk::Atom> = [
        "image/png",
        "image/bmp",
        "image/tiff",
        "image/jpeg",
        "image/x-bmp",
        "image/x-MS-bmp",
    ]
    .iter()
    .map(|s| gdk::Atom::intern(s))
    .collect();

    if targets.iter().any(|a| image_atoms.contains(a)) {
        if let Some(pixbuf) = clip.wait_for_image() {
            return Some(Content::Image(pixbuf));
        }
    }

    let text_atoms: Vec<gdk::Atom> = [
        "UTF8_STRING",
        "STRING",
        "TEXT",
        "text/plain;charset=utf-8",
        "text/plain",
    ]
    .iter()
    .map(|s| gdk::Atom::intern(s))
    .collect();

    if targets.iter().any(|a| text_atoms.contains(a)) {
        if let Some(text) = clip.wait_for_text() {
            if !text.is_empty() {
                return Some(Content::Text(text.to_string()));
            }
        }
    }

    None
}

pub fn set(clip: &Clipboard, content: &Content) {
    match content {
        Content::Text(t) => clip.set_text(t),
        Content::Image(p) => clip.set_image(p),
    }
}
