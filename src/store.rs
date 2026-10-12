use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::gdk_pixbuf;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClipItem {
    pub id: u64,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub created: i64,
    pub hash: u64,
}

impl ClipItem {
    pub fn is_image(&self) -> bool {
        self.kind == "image"
    }

    pub fn search_blob(&self) -> String {
        if self.is_image() {
            "image picture photo screenshot".to_string()
        } else {
            self.text.clone().unwrap_or_default()
        }
    }
}

pub enum Content {
    Text(String),
    Image(gdk_pixbuf::Pixbuf),
}

pub struct Paths {
    pub images: PathBuf,
    pub history: PathBuf,
    pub position: PathBuf,
}

pub fn paths() -> Paths {
    let base = data_dir().join("clipd");
    Paths {
        images: base.join("images"),
        history: base.join("history.json"),
        position: base.join("position.json"),
    }
}

#[derive(Serialize, Deserialize)]
struct Pos {
    x: i32,
    y: i32,
}

pub fn load_pos() -> Option<(i32, i32)> {
    let raw = fs::read_to_string(paths().position).ok()?;
    let pos: Pos = serde_json::from_str(&raw).ok()?;
    Some((pos.x, pos.y))
}

pub fn save_pos(x: i32, y: i32) {
    if let Ok(bytes) = serde_json::to_vec(&Pos { x, y }) {
        let _ = fs::write(paths().position, bytes);
    }
}

fn data_dir() -> PathBuf {
    if let Ok(x) = std::env::var("XDG_DATA_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x);
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local/share")
}

pub fn ensure_dirs() -> anyhow::Result<()> {
    let p = paths();
    fs::create_dir_all(&p.images)?;
    Ok(())
}

pub fn load() -> Vec<ClipItem> {
    let p = paths();
    match fs::read_to_string(&p.history) {
        Ok(s) => match serde_json::from_str::<Vec<ClipItem>>(&s) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("history.json is corrupt ({e}); starting with empty history");
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    }
}

pub fn save(items: &[ClipItem]) {
    let p = paths();
    if let Err(e) = fs::create_dir_all(&p.images) {
        log::warn!("cannot create data dir: {e}");
        return;
    }
    match serde_json::to_vec(items) {
        Ok(bytes) => {
            let tmp = p.history.with_extension("json.tmp");
            if let Err(e) = fs::write(&tmp, &bytes).and_then(|_| fs::rename(&tmp, &p.history)) {
                log::warn!("cannot write history: {e}");
            }
        }
        Err(e) => log::warn!("cannot serialize history: {e}"),
    }
}

pub fn record(items: &mut Vec<ClipItem>, content: &Content) -> bool {
    let created = now();

    match content {
        Content::Text(t) => {
            if t.is_empty() {
                return false;
            }
            let hash = hash_bytes(t.as_bytes());
            if bump_hash(items, "text", hash, created) {
                return false;
            }
            items.insert(
                0,
                ClipItem {
                    id: next_id(items),
                    kind: "text".into(),
                    text: Some(t.clone()),
                    image: None,
                    created,
                    hash,
                },
            );
            save(items);
            true
        }
        Content::Image(pb) => {
            let (hash, file) = match store_image(pb) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("cannot store image: {e}");
                    return false;
                }
            };
            if bump_hash(items, "image", hash, created) {
                return false;
            }
            items.insert(
                0,
                ClipItem {
                    id: next_id(items),
                    kind: "image".into(),
                    text: None,
                    image: Some(file),
                    created,
                    hash,
                },
            );
            save(items);
            true
        }
    }
}

pub fn content_of(item: &ClipItem) -> Option<Content> {
    if item.is_image() {
        let p = paths().images.join(item.image.clone()?);
        gdk_pixbuf::Pixbuf::from_file(&p).ok().map(Content::Image)
    } else {
        item.text.clone().map(Content::Text)
    }
}

pub fn bump(items: &mut Vec<ClipItem>, id: u64) {
    if let Some(pos) = items.iter().position(|i| i.id == id) {
        let mut it = items.remove(pos);
        it.created = now();
        items.insert(0, it);
        save(items);
    }
}

pub fn remove(items: &mut Vec<ClipItem>, id: u64) {
    if let Some(pos) = items.iter().position(|i| i.id == id) {
        let it = items.remove(pos);
        if it.is_image() {
            if let Some(name) = it.image {
                let _ = fs::remove_file(paths().images.join(name));
            }
        }
        save(items);
    }
}

pub fn clear(items: &mut Vec<ClipItem>) {
    let p = paths();
    if let Ok(entries) = fs::read_dir(&p.images) {
        for e in entries.flatten() {
            let _ = fs::remove_file(e.path());
        }
    }
    items.clear();
    save(items);
}

fn bump_hash(items: &mut Vec<ClipItem>, kind: &str, hash: u64, created: i64) -> bool {
    if let Some(pos) = items.iter().position(|i| i.kind == kind && i.hash == hash) {
        let mut it = items.remove(pos);
        it.created = created;
        items.insert(0, it);
        save(items);
        true
    } else {
        false
    }
}

fn next_id(items: &[ClipItem]) -> u64 {
    items.iter().map(|i| i.id).max().unwrap_or(0) + 1
}

fn store_image(pb: &gdk_pixbuf::Pixbuf) -> anyhow::Result<(u64, String)> {
    let bytes = pb.save_to_bufferv("png", &[])?;
    let hash = hash_bytes(&bytes);
    let name = format!("{hash:016x}.png");
    let dest = paths().images.join(&name);
    if !dest.exists() {
        fs::write(&dest, &bytes)?;
    }
    Ok((hash, name))
}

fn hash_bytes(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &c in b {
        h ^= c as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn human_age(ts: i64) -> String {
    let diff = (now() - ts).max(0);
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86_400 {
        format!("{}h ago", diff / 3600)
    } else if diff < 604_800 {
        format!("{}d ago", diff / 86_400)
    } else if diff < 2_592_000 {
        format!("{}w ago", diff / 604_800)
    } else if diff < 31_536_000 {
        format!("{}mo ago", diff / 2_592_000)
    } else {
        format!("{}y ago", diff / 31_536_000)
    }
}

pub fn images_dir() -> PathBuf {
    paths().images
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_sensitive() {
        assert_eq!(hash_bytes(b"hello"), hash_bytes(b"hello"));
        assert_ne!(hash_bytes(b"hello"), hash_bytes(b"hellp"));
        assert_ne!(hash_bytes(b""), hash_bytes(b"a"));
    }

    #[test]
    fn age_buckets() {
        let n = now();
        assert_eq!(human_age(n), "just now");
        assert_eq!(human_age(n - 120), "2m ago");
        assert_eq!(human_age(n - 3 * 3600), "3h ago");
        assert_eq!(human_age(n - 2 * 86_400), "2d ago");
        assert_eq!(human_age(n - 2 * 604_800), "2w ago");
        assert_eq!(human_age(n - 60 * 86_400), "2mo ago");
        assert_eq!(human_age(n - 400 * 86_400), "1y ago");
        assert_eq!(human_age(n + 10), "just now");
    }
}
