# Media Launcher

A controller-operated media application for local videos and phone-submitted
URLs, designed for Steam couch setups on Windows and Linux. Video plays inside
the application through libmpv. A separate-window mpv backend remains available.
Bundled dependencies and installers are still planned.

## Build and run

Install Rust 1.88 or newer with Cargo and a native C/C++ linker. On Windows,
use the MSVC Rust toolchain and Visual Studio Build Tools with the desktop C++
workload and Windows SDK. On Linux, install pkg-config and libudev development
files, plus your desktop's Wayland/X11, libxkbcommon, and OpenGL/EGL libraries.
The application supports both Wayland and X11. Cargo.lock pins Rust dependencies.

Install **libmpv (client API 2)** and **yt-dlp** separately for now. Linux builds
look for `libmpv.so.2` or `libmpv.so` beside the executable and then in the system
library search path. On Windows, place `mpv-2.dll` or `libmpv-2.dll` and its native
dependencies beside `media-launcher.exe`, matching its architecture. An explicit
`player.libmpv` path or `--libmpv PATH` can select another installation. Windows
also searches an explicit DLL's containing directory for its dependencies.

For separate-window playback, install mpv and select
`--player-backend external --mpv PATH` instead. A missing libmpv produces an
explanation in the app; it does not silently switch backends. Persistent player and extractor
settings, browser cookies, and plugin directories are described in
[configuration and customization](docs/configuration.md). Existing mpv
configuration and resume behavior remain in charge.

```sh
cargo build --locked --release
cargo run --locked -- --root /path/to/videos
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

The release executable is `target/release/media-launcher` on Linux and
`target\release\media-launcher.exe` on Windows. Copy it to a stable location,
or install it on PATH with `cargo install --locked --path .`.

Linux example:

```sh
media-launcher --root "$HOME/Videos" --ui-scale 1.5
```

Windows PowerShell example (using explicit paths):

```powershell
.\target\release\media-launcher.exe --root "$env:USERPROFILE\Videos" --libmpv "C:\Tools\mpv\mpv-2.dll" --ui-scale 1.5
```

On first launch, a default per-user `config.toml` and extractor directory are
created. Without a configured library root, the app waits for phone-submitted
URLs. Player defaults to embedded libmpv, the application window to fullscreen,
UI scale to `1.0`, and web listening to `0.0.0.0:8765`. Configuration persists between launches; CLI overrides apply
for one launch. Run `media-launcher --show-paths` to locate customization files.
Relative roots are accepted. `--mpv` accepts an executable path or a command on
PATH for the external backend. `--ui-scale` accepts `0.5`–`4.0` and multiplies
desktop display scaling.
Run `media-launcher --help` for syntax.

On Linux, an optional desktop entry is provided in
[packaging/media-launcher.desktop](packaging/media-launcher.desktop). Copy it to
`~/.local/share/applications/` after installing the executable on PATH. Its
working directory depends on the desktop; specify an absolute `--root` in its
`Exec` line to select your library reliably.

## Steam Big Picture

1. Add the installed executable as a non-Steam game in desktop Steam.
2. Name the shortcut **Media Launcher** and set **Start In** to the executable's
   containing directory.
3. Set **Launch Options** to `--root "PATH TO VIDEOS" --ui-scale 1.5`,
   substituting absolute paths for your platform. With embedded playback, omit
   `--mpv`; add `--libmpv "PATH TO LIBRARY"` if the library is elsewhere. With
   external playback, add `--player-backend external --mpv "PATH TO MPV"` to avoid
   depending on Steam's PATH. Adjust scaling for your TV.
4. Disable Steam Input for this shortcut to use native controller input.
5. On Linux, launch the native executable with compatibility/Proton disabled.

The launcher stays alive during playback so Steam tracks the whole session.
Embedded playback uses Media Launcher's controls in the same window. The external
backend uses the separate mpv installation's own playback/controller bindings.

## Phone URL submission

While the launcher is running, open `http://COMPUTER-LAN-IP:8765/` on a phone
on the same network and submit an HTTP or HTTPS media URL. Allow the application
through your firewall on the local network if needed. Only one playback can run
at a time. Use `--listen 127.0.0.1:8765` for access from this computer only, or
`--listen IP:PORT` to choose another interface or port. The current endpoint has
no authentication and is intended for a trusted local network.

## Browsing and controls

Lists immediate folders and videos, with folders first. No database, indexing,
or metadata fetching. Dot-prefixed names, broken symlinks, and symlinks outside
the library root are omitted. Supported extensions, case-insensitively:
`mp4 mkv m4v webm avi mov ts m2ts`. Native filesystem paths are retained separately
from display names.

| Controller (physical position) | Keyboard | Action |
| --- | --- | --- |
| D-pad / left stick up/down | Up / Down | Move, clamped at ends |
| A / south | Enter | Open folder / play; retry error |
| B / east | Escape | Parent folder / dismiss error |
| X / west | R | Refresh / retry |
| Start / menu | Q | Quit |

The stick dead zone is 0.35; repeat starts after 350 ms and repeats every 100 ms.
Controllers can reconnect. Selection and scroll are remembered per directory
for the session; refresh preserves the selected path where possible. Back at
root does nothing. The selected name wraps in the footer.

Embedded playback controls:

| Controller | Keyboard | Action |
| --- | --- | --- |
| A / south | Enter / Space | Play/pause |
| B / east | Escape | Stop and return to launcher |
| D-pad / left stick left/right | Left / Right | Seek −10 / +10 seconds per press |
| D-pad / left stick up/down | Up / Down | Volume up/down |
| Start / menu | Q | Exit application |
| — | F | Toggle application fullscreen (also works in the browser) |

On-screen buttons support mouse input. EOF and playback errors return to the
launcher with directory selection preserved. Back honors mpv's
`save-position-on-quit` preference by writing its watch-later state before stop.
Window closing during embedded playback shuts down the renderer/player cleanly.
After playback and at startup, controls must be neutral for 200 ms before new
presses are accepted. Controller remapping and richer player menus are later work.

With **external** playback, launcher actions and close requests remain disabled
until mpv exits, and its own bindings handle playback. The separate mpv window
stays above the launcher according to your desktop's window management.

## Errors and diagnostics

Unavailable directories, removed files, failed spawns, and failed player exits
leave the browser usable. Retry with X, dismiss with B, then B again to go back.
The application does not change media ownership or permissions.

External playback writes stdout/stderr per playback; embedded playback writes
libmpv diagnostics per application session. Both use a file named
`media-launcher-<pid>-<timestamp>.log` in the operating system's temporary
directory. Unix logs are created with mode `0600`; Windows logs inherit the
temporary directory's access permissions. The log path appears on stderr and
in unsuccessful-exit errors. Launcher diagnostics also go to stderr.

## Validation

See [validation notes](docs/validation.md) for checks actually performed and
remaining hardware checks. The native Linux rendering/lifecycle test has passed. Embedded playback on
Windows, Steam/controller acceptance, hardware decoding, and Linux Wayland
behavior still require target-machine testing.

The [original v0.1 spec](docs/media-launcher-spec.md) records the historical
single-machine design. Its Nix packaging and machine-specific paths no longer
apply to the current project.
