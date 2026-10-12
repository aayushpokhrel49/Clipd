# clipd

A small, fast, persistent clipboard manager for **X11** desktops. It runs on any
X11 Linux distribution and works with any window manager or desktop environment
(GNOME, KDE Plasma, XFCE, Cinnamon, MATE, LXQt, i3, Sway-on-XWayland, ...). It
watches your clipboard, saves everything you copy — text **and images** — to disk,
and lets you pop up a searchable list whenever you want to paste something you
copied earlier.

- Event-driven (no polling), light on RAM/CPU.
- Saves history to disk, so it survives app shutdowns and reboots.
- Captures **text and images** (PNG, JPEG, GIF, WebP, BMP, TIFF, ...).
- Global hotkey **Ctrl+Alt+V** opens a searchable popup; click / Enter to paste.
- **Auto-pastes** into the focused window after you pick an item — using
  **Ctrl+Shift+V** in terminals and **Ctrl+V** everywhere else.
- Full keyboard navigation: arrows, Page Up/Down, Home/End, Delete, Esc.
- Skips clipboard content flagged by password managers.

## Requirements

`clipd` uses GTK3 and X11 directly; there is no runtime dependency on `xclip`,
`xsel`, `xdotool`, or a GUI toolkit beyond the system GTK3 libraries. The
optional self-updater shells out to `curl` (and `sha256sum`); updates are skipped
if those are missing.

- A running **X11** session (not native Wayland — see limitations below).
- GTK3 runtime — present on essentially every desktop.

Building from source additionally needs the GTK3 development headers and a Rust
toolchain (`cargo --version`, 1.70+ recommended):

| Distribution       | Command                                                                 |
|--------------------|-------------------------------------------------------------------------|
| Debian / Ubuntu / Mint | `sudo apt install build-essential pkg-config libgtk-3-dev libglib2.0-dev libgdk-pixbuf-2.0-dev libcairo2-dev` |
| Fedora / RHEL      | `sudo dnf install gcc pkg-config gtk3-devel glib2-devel gdk-pixbuf2-devel cairo-devel` |
| Arch / Manjaro     | `sudo pacman -S base-devel pkgconf gtk3 glib2 gdk-pixbuf2 cairo`        |
| openSUSE           | `sudo zypper install gcc pkg-config gtk3-devel glib2-devel gdk-pixbuf-devel cairo-devel` |
| Alpine             | `sudo apk add build-base pkgconf gtk+3.0-dev glib-dev gdk-pixbuf-dev cairo-dev` |

## Build & install

```
./install.sh
```

This builds a release binary, installs it to `~/.local/bin/clipd`, and adds an
autostart entry at `~/.config/autostart/clipd.desktop`.

Manual build:

```
cargo build --release
./target/release/clipd start
```

## Usage

| Command                  | What it does                                   |
|--------------------------|------------------------------------------------|
| `clipd start`            | Run the background daemon (hidden).            |
| `clipd` / `clipd toggle` | Show or hide the clipboard popup.              |
| `clipd show`             | Show the popup.                                |
| `clipd clear`            | Delete the whole history.                      |
| `clipd update`           | Check GitHub and install the latest release.   |
| `clipd quit`             | Stop the daemon.                               |

Once the daemon is running:

- **Copy anything** (Ctrl+C). It is appended to history and persisted.
- Press **Ctrl+Alt+V** anywhere to open the popup near the mouse pointer
  (works regardless of whether NumLock/CapsLock/ScrollLock are on).
- Type to filter. Use **Up/Down**, **Page Up/Down**, **Home/End** to move,
  **Enter** or **click** to choose, **Delete** to remove an entry, **Esc** to close.
- Choosing an entry puts it on the clipboard, restores focus to the previous
  window, and sends **Ctrl+Shift+V** to terminals or **Ctrl+V** to other apps.
- Drag the popup header to reposition it; its location is remembered.

Only one instance runs at a time: launching `clipd` again talks to the running
daemon (over the session D-Bus) and toggles the popup.

## Where data is stored

```
~/.local/share/clipd/history.json     # metadata (text, image filenames, hashes)
~/.local/share/clipd/images/*.png     # copied images
```

History is unlimited by design. Use `clipd clear` to wipe it, or delete the
`~/.local/share/clipd` directory.

## Configuration

`clipd` works with sensible defaults and needs no configuration. The Ctrl+Alt+V
binding is grabbed by `clipd` itself while it runs, so you do not need to create a
desktop shortcut manually.

## Updates

`clipd` updates itself from the project's GitHub releases
(`aayushpokhrel49/Clipd`). Shortly after start-up, and then every 6 hours, it
asks GitHub for the newest release; if it is newer than the running version, the
binary is downloaded, checksum-verified, swapped in place, and the daemon
restarts automatically. A desktop notification reports what happened.

- `clipd update` forces an update check right away.
- Auto-update is skipped when running a development build (from `target/`), so
  `cargo run` and test binaries are never replaced. `clipd update` still works.
- Set `CLIPD_NO_UPDATE=1` to turn updates off completely.
- Set `CLIPD_REPO=owner/repo` to point at a fork.
- The updater uses `curl` (and `sha256sum` when the release ships a
  `clipd.sha256`) and fails quietly if they are missing.

### Publishing a release

Tag a commit and push the tag; the workflow in `.github/workflows/release.yml`
builds `x86_64` and `aarch64` binaries and attaches them to a GitHub release:

```
# bump `version` in Cargo.toml first
git tag v0.1.1
git push origin v0.1.1
```

The updater downloads the asset matching the running CPU architecture
(`clipd-x86_64` / `clipd-aarch64`). The legacy `clipd` asset is also kept so
older install scripts keep working.

## Uninstall

```
clipd quit
rm -f ~/.local/bin/clipd ~/.config/autostart/clipd.desktop
rm -rf ~/.local/share/clipd
```

## Notes & limitations

- **X11 based.** It works on native X11 sessions and on Wayland desktops through
  XWayland (e.g. GNOME, KDE Plasma and Cinnamon run GTK apps under XWayland).
  Native Wayland-only apps do not expose their clipboard through the X11
  protocol and will not be captured.
- Auto-paste is best-effort: it asks the window manager to focus the previous
  window (`_NET_ACTIVE_WINDOW`), falls back to a direct focus request, then
  synthesizes the right paste shortcut. If a target app ignores synthetic input,
  the item is still on the clipboard — just press Ctrl+V yourself.
- If the **Ctrl+Alt+V** grab is already taken by another application, the
  clipboard watcher still runs; just use `clipd toggle` to open the popup.
- If another clipboard manager is running, the two may compete for clipboard
  ownership.
- The popup builds at most 400 matching rows at a time; type more of the text
  to narrow the search and reach older entries. The popup automatically shrinks
  to fit small screens and high display scaling.
