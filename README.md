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
- **Category tabs** to split the history: All, Images, URL, Shorts, Large
  (**Ctrl+1..5**), combinable with the search box.
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

## Install

Prebuilt packages are attached to every [release](https://github.com/aayushpokhrel49/Clipd/releases).
Replace `VERSION`/`amd64` with the version and architecture you need (`amd64` for
Intel/AMD 64-bit, `arm64` for ARM 64-bit). Packages install the binary to
`/usr/bin/clipd` plus a system autostart entry, so the daemon starts on next
login for every user.

**Debian / Ubuntu / Linux Mint / Pop!_OS / Kali / Raspberry Pi OS** (`.deb`):

```
curl -LO https://github.com/aayushpokhrel49/Clipd/releases/latest/download/clipd_VERSION-1_amd64.deb
sudo apt install ./clipd_VERSION-1_amd64.deb
```

**Fedora / RHEL / CentOS / Rocky / AlmaLinux / openSUSE** (`.rpm`):

```
sudo dnf install https://github.com/aayushpokhrel49/Clipd/releases/latest/download/clipd-VERSION-1.x86_64.rpm
# openSUSE: sudo zypper install --allow-unsigned-rpm <same-url>
```

**Arch / Manjaro / EndeavourOS** (`.pkg.tar.zst`):

```
sudo pacman -U https://github.com/aayushpokhrel49/Clipd/releases/latest/download/clipd-VERSION-1-x86_64.pkg.tar.zst
```

**Alpine** (`.apk`):

```
sudo apk add --allow-untrusted https://github.com/aayushpokhrel49/Clipd/releases/latest/download/clipd_VERSION-r1_x86_64.apk
```

Swap `x86_64` for `aarch64` (and `.rpm` arch for `aarch64`) on ARM machines.
Start it right away with `clipd start &` (or just log out and back in).

### One-line source install (any distro)

If there is no package for your distribution, the script builds from source and
installs to `~/.local/bin` with a per-user autostart entry:

```
./install.sh
```

Manual build:

```
cargo build --release
./target/release/clipd start
```

### Building packages yourself

With [`nfpm`](https://nfpm.goreleaser.com/install/) on your `PATH`:

```
./packaging/build-packages.sh          # builds .deb/.rpm/Arch/.apk into dist/
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
- Switch category with the tabs (or **Ctrl+1..5**): **All** (everything),
  **Images** (pictures only), **URL** (links only), **Shorts** (text under 500
  characters) and **Large** (500 characters or more). Filtering combines with the
  search box, so you can e.g. open **URL** and type part of a link.
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
- Auto-update is also skipped for **package-managed** installs (binary under
  `/usr`, `/nix/store`, `/snap` or Flatpak). Update those through your package
  manager (`apt upgrade`, `dnf upgrade`, `pacman -Syu`, `apk upgrade`, ...) so
  the system copy stays consistent.
- Set `CLIPD_NO_UPDATE=1` to turn updates off completely.
- Set `CLIPD_REPO=owner/repo` to point at a fork.
- The updater uses `curl` (and `sha256sum` when the release ships a
  `clipd.sha256`) and fails quietly if they are missing.

### Publishing a release

Tag a commit and push the tag; the workflow in `.github/workflows/release.yml`
builds `x86_64` and `aarch64` binaries **and `.deb` / `.rpm` / Arch / Alpine
packages**, then attaches everything to a GitHub release:

```
# bump `version` in Cargo.toml first
git tag v0.2.0
git push origin v0.2.0
```

The updater downloads the asset matching the running CPU architecture
(`clipd-x86_64` / `clipd-aarch64`). The legacy `clipd` asset is also kept so
older install scripts keep working.

## Uninstall

Package install:

```
sudo apt remove clipd      # or: sudo dnf remove clipd / sudo pacman -R clipd / sudo apk del clipd
rm -rf ~/.local/share/clipd
```

Script / manual install:

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
