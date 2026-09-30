# Configuration and customization

Media Launcher reads `config.toml` at startup. On first launch, it creates a
commented default file if none exists at the standard location. It preserves
existing files. Restart the application after changing settings. Invalid TOML,
unknown fields, invalid UI scale, and unavailable extra plugin directories
produce startup errors with diagnostic details.

Run `media-launcher --show-paths` to print the exact locations for your account.
This command and `--help` do not create files or start the GUI or web server.

| Platform | Configuration | User data |
| --- | --- | --- |
| Linux | `$XDG_CONFIG_HOME/media-launcher/config.toml` (default `~/.config/media-launcher/config.toml`) | `$XDG_DATA_HOME/media-launcher` (default `~/.local/share/media-launcher`) |
| Windows | `%APPDATA%\MediaLauncher\config.toml` | `%APPDATA%\MediaLauncher` |

`--config PATH` reads a different, existing configuration file. Copy
[the default template](../packaging/config.toml) to that path first.
`--data-dir PATH` changes the user data location. This can be useful for a portable
installation. Changing the configuration file alone does not change user data.

## Settings

```toml
# Optional. Omit this line for URL-only use.
root = '/home/you/Videos'
ui_scale = 1.5
listen = '0.0.0.0:8765'

[window]
mode = 'fullscreen'
size = [1280, 720]
decorations = true
resizable = true

[player]
backend = 'embedded'
# libmpv = '/path/to/libmpv.so.2'
args = ['--hwdec=auto']

[yt_dlp]
executable = 'yt-dlp'
plugin_dirs = ['/home/you/my-yt-dlp-plugins']
cookies_from_browser = 'firefox'
args = ['--extractor-args', 'youtube:player_client=default']
```

All fields are optional. Defaults are: no local library, UI scale `1.0`, listen
address `0.0.0.0:8765`, a fullscreen application window, embedded libmpv, and no
extra arguments. For the external backend, mpv defaults to PATH and fullscreen.
Omitting `yt_dlp.executable` selects `yt-dlp.exe` beside the application on Windows
when present, otherwise leaving mpv's normal executable discovery in charge.
With the bundled extractor, the app also supplies the adjacent Deno path unless
`args` already specifies `--js-runtimes`. Explicit extractor paths remain
user-managed and bypass this bundle configuration.
The app's plugin directory and yt-dlp's default plugin discovery are included.
Install a current yt-dlp for plugin support; youtube-dl does not support this
customization setup. The Windows bundle includes these dependencies; the Arch
package uses system packages. Plain Cargo builds do not download runtime tools.

Use TOML literal strings (single quotes) for Windows paths:

```toml
root = 'C:\Users\You\Videos'
[player]
backend = 'embedded'
libmpv = 'C:\Tools\mpv\mpv-2.dll'
[yt_dlp]
executable = 'C:\Tools\yt-dlp\yt-dlp.exe'
```

Relative `root`, `player.libmpv`, executable paths containing directory
components, and `plugin_dirs` resolve relative to `config.toml`. Bare executable names resolve on
PATH. Use `./mpv` or `.\mpv.exe` to select an executable beside the configuration.
These fields do not expand `~` or environment variables. Additional arguments
are passed verbatim; use absolute paths in arguments that refer to files.

`player.args` accepts one mpv option per entry, using `--option=value` when a
value is needed. The app never executes a shell command. Embedded playback loads
normal mpv configuration, applies the supplied options, then enforces the
application-owned video output, idle/EOF behavior, input handling, player overlay,
and diagnostic log. Playback-window mpv options do not control the application
window. List operations such as `--script-opts-append=key=value` are supported.
Startup-only options (`config`, `config-dir`, `input-conf`, `load-scripts`, `scripts`,
`player-operation-mode`, `input-app-events`) are supplied before libmpv reads its
configuration. Use normal script directories or `--scripts=...` to load mpv scripts.
Options specific to the command-line player may be incompatible with embedding;
initialization errors are displayed rather than ignored.

With `backend = 'external'`, `player.executable` chooses mpv and
`player.fullscreen` controls its separate window. Extra options follow that
fullscreen setting and can override it. These two fields have no effect on
embedded playback. Existing configuration files remain valid, but omitted
`backend` now selects embedded playback. Use `--player-backend external` for the
previous behavior; `--mpv` alone does not change the backend.

`yt_dlp.args` accepts separate command-line argument entries, including repeated
options. `cookies_from_browser` uses yt-dlp's normal browser/profile syntax.
An existing yt-dlp configuration can be added with, for example,
`args = ['--config-locations', '/absolute/path/to/yt-dlp.conf']`.

mpv still invokes yt-dlp itself. Media Launcher writes a private temporary
UTF-8 yt-dlp configuration and passes its location to mpv. In embedded mode, it
lives for the player session; in external mode, it lives for one playback. Normal mpv and yt-dlp user
configuration still loads unless explicitly disabled by your options. The
launcher supplies the `config-locations` raw option, so an existing mpv setting
for that key is replaced; use `yt_dlp.args` to include those extra files.

## Application window

`window.mode` accepts `fullscreen` (the default), `windowed`, or `maximized`.
`window.size` is the initial windowed size in logical pixels; both dimensions
must be finite and between 320 and 16384. Desktop scaling applies independently
of `ui_scale`. `decorations` and `resizable` default to `true`. Windowed startup
requests a centered window. Fullscreen and maximized startup use the monitor's
available dimensions, without applying the configured windowed size afterward.

These settings apply to the application on both platforms, regardless of the
playback backend. F toggles fullscreen for the current session without changing
saved configuration; leaving fullscreen restores the configured windowed size.
Window-manager support determines the effect of decoration
and resize requests; the app does not force compositor-specific window placement
or focus. Monitor selection, saved geometry, and exclusive display modes are not
part of these settings.

## Library loading

The embedded backend needs libmpv client API 2 with OpenGL render API support,
matching the application's architecture. It loads dynamically, so compiling does
not require libmpv headers or linker import libraries. On Windows, library lookup
checks `mpv-2.dll` and `libmpv-2.dll`; on Linux it checks `libmpv.so.2` and `libmpv.so`.
It checks the executable directory first, then the OS library search path.
`player.libmpv` and `--libmpv` select an explicit file. A bad explicit path never
falls back to a different library. Native dependencies must also be available;
Windows searches beside an explicitly selected DLL for those dependencies.

A missing library or an initialization failure leaves the launcher visible with
an error. Select the external backend explicitly if needed; there is no silent
backend switch. See [Windows packaging](../packaging/windows/README.md) for the
initial bundle and installer build instructions.

## Custom extractors

First launch creates this directory under user data:

```text
yt-dlp/
└── plugins/
    └── user/
        └── yt_dlp_plugins/
            └── extractor/
                └── my_extractor.py       # supplied by you
```

Place your extractor in the directory printed by `--show-paths`. It stays outside
the application installation and is never generated or overwritten by Media
Launcher. Use ordinary yt-dlp extractor classes; no Media Launcher plugin API is
required. Additional bundles can be placed beside `user` under `plugins`.

Every extra `plugin_dirs` entry is also a search directory whose children are
plugin bundles containing `yt_dlp_plugins`. For example:

```text
my-yt-dlp-plugins/                         # plugin_dirs entry
└── my_bundle/
    └── yt_dlp_plugins/
        └── extractor/
            └── my_extractor.py
```

This follows [yt-dlp's plugin discovery conventions](https://github.com/yt-dlp/yt-dlp#plugins).
The yt-dlp [configuration documentation](https://github.com/yt-dlp/yt-dlp#configuration)
and [mpv's yt-dlp integration options](https://mpv.io/manual/stable/#options-ytdl)
explain the underlying behavior.

## Command-line precedence

Command-line values override configuration values for that launch without
writing them back. Supported overrides:

- `--root PATH`
- `--mpv EXECUTABLE` (external backend)
- `--player-backend embedded|external`
- `--libmpv PATH`
- `--window-mode fullscreen|windowed|maximized`
- `--yt-dlp EXECUTABLE`
- `--ui-scale NUMBER`
- `--listen IP:PORT`
- `--yt-dlp-plugin-dir PATH` (repeatable)

If any `--yt-dlp-plugin-dir` values are supplied, they replace the configured
extra `plugin_dirs`; the application's own directory remains included. Relative
command-line paths resolve against the current working directory.

Without a library root, the app shows a waiting screen and accepts phone URL
submissions. With a root, it also shows the existing filesystem browser. Playback
and customization use the same settings for local files and submitted URLs.
