# Media Launcher

A fullscreen, controller-operated folder browser that plays local videos and
phone-submitted URLs through your existing mpv installation. Windows and Linux
are the intended platforms. Playback currently opens a separate mpv window;
bundled dependencies and embedded playback are planned, not implemented.

## Build and run

Install Rust 1.88 or newer with Cargo and a native C/C++ linker. On Windows,
use the MSVC Rust toolchain and Visual Studio Build Tools with the desktop C++
workload and Windows SDK. On Linux, install pkg-config and libudev development
files, plus your desktop's Wayland/X11, libxkbcommon, and OpenGL/EGL libraries.
The application supports both Wayland and X11. Cargo.lock pins Rust dependencies.

Install mpv separately for now. For web URLs, configure mpv with yt-dlp and any
custom extractor plugins you use. Existing mpv configuration and resume behavior
remain in charge.

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
media-launcher --root "$HOME/Videos" --mpv mpv --ui-scale 1.5
```

Windows PowerShell example (using explicit paths):

```powershell
.\target\release\media-launcher.exe --root "$env:USERPROFILE\Videos" --mpv "C:\Tools\mpv\mpv.exe" --ui-scale 1.5
```

Defaults: library root is the **current working directory**, player is `mpv`
found on PATH, UI scale is `1.0`, and the web server listens on `0.0.0.0:8765`.
Relative roots are accepted. `--mpv` accepts an executable path or a command on
PATH. `--ui-scale` accepts `0.5`–`4.0` and multiplies desktop display scaling.
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
3. Set **Launch Options** to `--root "PATH TO VIDEOS" --mpv "PATH TO MPV" --ui-scale 1.5`,
   substituting absolute paths for your platform. An explicit mpv path avoids
   depending on Steam's PATH. Adjust scaling for your TV.
4. Disable Steam Input for this shortcut to use native controller input.
5. On Linux, launch the native executable with compatibility/Proton disabled.

The launcher stays alive during playback so Steam tracks the whole session.
The separate mpv installation must also support your controller bindings;
launcher-owned playback controls will arrive with embedded playback.

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

During playback, launcher actions and window-close requests are disabled.
mpv owns playback input. After playback and at startup, controls must be neutral
for 200 ms before new presses are accepted. The launcher remains underneath mpv;
no compositor-specific focus requests are issued. It sleeps when idle and checks
playback without blocking the UI.

## Errors and diagnostics

Unavailable directories, removed files, failed spawns, and failed player exits
leave the browser usable. Retry with X, dismiss with B, then B again to go back.
The application does not change media ownership or permissions.

Each playback writes stdout/stderr to a new file named
`media-launcher-<pid>-<timestamp>.log` in the operating system's temporary
directory. Unix logs are created with mode `0600`; Windows logs inherit the
temporary directory's access permissions. The log path appears on stderr and
in unsuccessful-exit errors. Launcher diagnostics also go to stderr.

## Validation

See [validation notes](docs/validation.md) for checks actually performed and
remaining hardware checks. Windows runtime behavior and Steam/controller
acceptance require testing on Windows; Linux Wayland/Steam/controller behavior
also requires real hardware validation.

The [original v0.1 spec](docs/media-launcher-spec.md) records the historical
single-machine design. Its Nix packaging and machine-specific paths no longer
apply to the current project.
