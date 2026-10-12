# clipd

A small, fast, persistent clipboard manager for **X11** desktops (built and tested on
Linux Mint / Cinnamon). It watches your clipboard, saves everything you copy — text
**and images** — to disk, and lets you pop up a searchable list whenever you want to
paste something you copied earlier.

- Event-driven (no polling), light on RAM/CPU.
- Saves history to disk, so it survives app shutdowns and reboots.
- Captures **text and images**.
- Global hotkey **Ctrl+Alt+V** opens a searchable popup; click / Enter to paste.
- **Auto-pastes** into the focused window after you pick an item.
- Skips clipboard content flagged by password managers.

## Requirements

`clipd` uses GTK3 and X11 directly; there is no runtime dependency on `xclip`,
`xsel`, `xdotool`, or a GUI toolkit beyond the system GTK3 libraries. The
optional self-updater shells out to `curl` (and `sha256sum`); updates are skipped
if those are missing.

- A running **X11** session (not Wayland).
- GTK3 runtime (`libgtk-3-0`) — present on essentially every desktop.

Building from source additionally needs the GTK3 development headers and a Rust
toolchain:

```
sudo apt install build-essential pkg-config libgtk-3-dev libglib2.0-dev \
                 libgdk-pixbuf-2.0-dev libcairo2-dev
cargo --version   # 1.70+ recommended
```

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
- Press **Ctrl+Alt+V** anywhere to open the popup near the mouse pointer.
- Type to filter. Use **Up/Down** to move, **Enter** or **click** to choose,
  **Delete** to remove an entry, **Esc** to close.
- Choosing an entry puts it on the clipboard, restores focus to the previous
  window, and sends **Ctrl+V** for you.

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
builds the binary and attaches `clipd` + `clipd.sha256` to a GitHub release:

```
# bump `version` in Cargo.toml first
git tag v0.1.1
git push origin v0.1.1
```

## Uninstall

```
clipd quit
rm -f ~/.local/bin/clipd ~/.config/autostart/clipd.desktop
rm -rf ~/.local/share/clipd
```

## Notes & limitations

- **X11 only.** There is no Wayland session support in this version.
- Native Wayland apps do not expose their clipboard through the X11 protocol and
  will not be captured.
- Auto-paste is best-effort: it restores focus to the window that was focused when
  the popup opened, then synthesizes Ctrl+V. If a target app ignores synthetic
  input, the item is still on the clipboard — just press Ctrl+V yourself.
- If another clipboard manager is running, the two may compete for clipboard
  ownership.
- The popup builds at most 400 matching rows at a time; type more of the text
  to narrow the search and reach older entries.
