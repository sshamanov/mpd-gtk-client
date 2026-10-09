# mpd-gtk-client-rs

An album-centric desktop client for [MPD](https://www.musicpd.org/) (Music Player Daemon), written in Rust with GTK4 and libadwaita.

## Features

- **Album mode** — cover grid with hover controls, grouped views and instant local search (no MPD round-trip per keystroke)
- **Folder mode** — directory browser with breadcrumbs and CUE sheet / DSD detection
- **Queue** — drag-and-drop reordering, keyboard move and remove
- **Cover art** — MPD `albumart` / `readpicture` with a disk cache; optional online lookup via MusicBrainz and the Cover Art Archive
- **MPRIS** — desktop media keys and shell integration over D-Bus (optional feature)
- **Keyboard-first** — global shortcuts, shortcuts overlay (`Ctrl+?`)
- No async runtime: plain `std::thread` workers for MPD IO, cover fetching and search

## Build

Requires Rust 1.85+, GTK 4.14+ and libadwaita 1.7+.

```sh
cargo build --release
cargo build --release --features online-cover-art,mpris   # optional extras
cargo test
```

## Usage

```sh
mpd-client                                  # connects to 127.0.0.1:6600
mpd-client --mpd-host 192.168.1.100 --mode folder
mpd-client --toggle-playback                # media action, then exit
```

Run `mpd-client --help` for all options. Settings are stored in `~/.config/mpd-client/config.toml`.

| Key | Action |
|-----|--------|
| `Space` | Play / pause |
| `Ctrl+F` | Search |
| `Enter` | Play selected album |
| `Shift+↑` / `Shift+↓` | Move queue item |
| `Del` | Remove from queue |
| `Ctrl+,` | Settings |
| `Ctrl+Q` | Quit |

## License

MIT
