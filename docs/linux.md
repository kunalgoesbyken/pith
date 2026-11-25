# Linux (X11, experimental)

Run `pith --allow-accessibility` in an X11 session. It refuses to start in a Wayland session. Desktop accessibility must be on; the flag switches it on for the session and `SIGINT` or `SIGTERM` (Ctrl+C, a kill, a logout) restores it. A process that is killed hard leaves it on.

- **Reads text** through AT-SPI from the focused field only. The accessibility bus delivers focus, caret and selection events from every app, but they carry object references and no text; Pith never asks any other field for its text.
- **Captures the accept keys and Esc** with an X key grab only while a suggestion is shown, plus the force-suggestion chord while a field Pith may read is focused. A key it does not act on is handed back to the app unchanged; if Pith does not answer within 150 ms the key goes to the app.
- **Draws** the suggestion in an override-redirect window that never takes the focus or the mouse. With a compositing manager it is inline at the caret, in the field's font and colour at reduced opacity. Without one it is a pill below the caret.
- **Inserts** through the accessibility interface. It falls back to the clipboard only when the field has no editable text interface or refuses the insert. Once a field has accepted an insert Pith never pastes, even if the text cannot be confirmed; it re-reads the field once and takes whatever is there. Firefox, Thunderbird, LibreWolf and Tor Browser accept the insert call but ignore it, so Pith pastes in them from the start. Before either path Pith checks that the focused field and app are still the ones the suggestion was made for.
- **Config and data** live in `~/.config/pith/config.toml` and `~/.local/share/pith/` (the model, `pith.log` and `style.toml` when `style_memory = true`), or under `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME`. `blocked_apps` and `allowed_apps` use the lower-case program name, for example `keepassxc`.
- **Tray** is a StatusNotifier item, so it shows wherever the panel implements `org.kde.StatusNotifierWatcher`. It lists the status line, the pause choices, Resume, a switch for the app you last typed in, Start at login, Open settings and Quit. On GNOME it needs the AppIndicator extension (Ubuntu ships it; Fedora and stock GNOME do not). CI exercises the item against a stand-in watcher and notification server, not against a real KDE, Xfce or GNOME panel.
- **Without a tray** (no watcher on the session bus) Pith still runs. It tries again for about 30 seconds in case the panel starts late, then says so in a desktop notice the first time, and `pith --settings`, or simply starting `pith` again, reaches the running instance, opens the settings file and shows the current status as a notice.
- **One instance per session**: Pith owns `org.pith.Pith` on the session bus. A second launch asks the running instance to open its settings and exits.
- **Settings** has no window yet. Open settings opens `config.toml` with `xdg-open` (it is created with the defaults first if it is missing). Edits to the file apply the next time Pith starts; pausing and the per-app switch from the tray apply at once and are saved.
- **Start at login** writes or removes `~/.config/autostart/pith.desktop` (under `$XDG_CONFIG_HOME`), with the path of the running binary and `--allow-accessibility` if you started with it. While the file exists Pith checks it at each start. If the program in its `Exec=` line has moved, Pith swaps in the new path and keeps your arguments, your edits and every other line. A file it cannot read is never overwritten, and an entry the desktop has disabled (`Hidden=true` or `X-GNOME-Autostart-enabled=false`) stays off.
- **Notices** go through `org.freedesktop.Notifications`: first run, a model that fails to load, and the status when there is no tray.
- **Install** from the release: `pith-linux-x86_64.tar.gz`, built on Ubuntu 22.04 so it runs on glibc 2.35 and newer. `sha256sum -c --ignore-missing SHA256SUMS`, unpack, run `pith --allow-accessibility`.
- **Build** with `cargo build --release -p pith`. CI runs the end-to-end suite on Xvfb with GTK 3 and Firefox.

Which app is behind a window comes from the process that owns it. Programs run by an interpreter or a sandbox are named as follows, and Pith reads nothing from a window it cannot name:

- Python, Perl, Ruby, Node, Mono, .NET, shell scripts: the script or assembly name, after skipping the interpreter's own options and their values. An option Pith does not know, `-c`, `-m`, `-e`, and a generic entry name (`main`, `app`, `index`, `__main__`, `run`, `start`, `server`, `cli`) give no name, so nothing is read.
- Java: only `-jar` gives a name; a main class does not, so nothing is read.
- Flatpak and Snap: the full application id (`FLATPAK_ID`, or `snap.<name>` for `SNAP_NAME`), matched through a table of known ids such as `org.keepassxc.KeePassXC` to `keepassxc`. An id that is not in the table is off by default. Pith writes it to `pith.log` once; put that exact id in `allowed_apps` to turn it on. Blocked apps and password fields never have their window title read either.
- Apps seen only through the sandbox bridge (`bwrap`, `flatpak-spawn`, `xdg-dbus-proxy`) have no name, so nothing is read.
- A shared Electron binary opening an `.asar` has no name, so nothing is read. An Electron app that ships its own executable is named after it.

Known gaps:

- No settings window yet (Phase 2); no battery-saver pause, no rival-tool detection and no update notice on Linux.
- GTK 4 does not report the caret position, so there is no placement in GTK 4 fields.
- The inline ghost wraps against the monitor, not the field.
