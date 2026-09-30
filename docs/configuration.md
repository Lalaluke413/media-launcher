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

[player]
executable = 'mpv'
fullscreen = true
args = ['--hwdec=auto']

[yt_dlp]
executable = 'yt-dlp'
plugin_dirs = ['/home/you/my-yt-dlp-plugins']
cookies_from_browser = 'firefox'
args = ['--extractor-args', 'youtube:player_client=default']
```

All fields are optional. Defaults are: no local library, UI scale `1.0`, listen
address `0.0.0.0:8765`, fullscreen playback, `mpv` on PATH, and no extra arguments.
Omitting `yt_dlp.executable` leaves mpv's normal executable discovery in charge.
The app's plugin directory and yt-dlp's default plugin discovery are included.
Install a current yt-dlp for plugin support; youtube-dl does not support this
customization setup. Neither mpv nor yt-dlp is bundled yet.

Use TOML literal strings (single quotes) for Windows paths:

```toml
root = 'C:\Users\You\Videos'
[player]
executable = 'C:\Tools\mpv\mpv.exe'
[yt_dlp]
executable = 'C:\Tools\yt-dlp\yt-dlp.exe'
```

Relative `root`, executable paths containing directory components, and
`plugin_dirs` resolve relative to `config.toml`. Bare executable names resolve on
PATH. Use `./mpv` or `.\mpv.exe` to select an executable beside the configuration.
These fields do not expand `~` or environment variables. Additional arguments
are passed verbatim; use absolute paths in arguments that refer to files.

`player.args` accepts one mpv option per entry, using `--option=value` when a
value is needed. These options follow the `fullscreen` setting and can override
it. The app appends its yt-dlp integration settings and then the selected media
as a separate native argument. It never executes a shell command.

`yt_dlp.args` accepts separate command-line argument entries, including repeated
options. `cookies_from_browser` uses yt-dlp's normal browser/profile syntax.
An existing yt-dlp configuration can be added with, for example,
`args = ['--config-locations', '/absolute/path/to/yt-dlp.conf']`.

mpv still invokes yt-dlp itself. Media Launcher writes a private temporary
UTF-8 yt-dlp configuration for each playback and passes its location to mpv.
The temporary file is removed once playback ends. Normal mpv and yt-dlp user
configuration still loads unless explicitly disabled by your options. The
launcher supplies the `config-locations` raw option, so an existing mpv setting
for that key is replaced; use `yt_dlp.args` to include those extra files.

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
- `--mpv EXECUTABLE`
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
