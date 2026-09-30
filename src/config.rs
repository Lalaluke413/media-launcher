use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub const TEMPLATE: &str = include_str!("../packaging/config.toml");
const HELP: &str =
    "media-launcher [--config PATH] [--data-dir PATH] [--root PATH] [--mpv EXECUTABLE]
               [--yt-dlp EXECUTABLE] [--yt-dlp-plugin-dir PATH] [--ui-scale NUMBER]
               [--listen IP:PORT] [--show-paths]
               [--player-backend embedded|external] [--libmpv PATH]
               [--window-mode fullscreen|windowed|maximized]
Settings are read from the per-user config.toml; command-line values override them.
No library root is required. Defaults: embedded libmpv, fullscreen window, UI scale 1.0, 0.0.0.0:8765.
UI scale must be between 0.5 and 4.0. Repeat --yt-dlp-plugin-dir for multiple paths.
--show-paths prints customization locations without creating files.
--mpv sets the external executable; choose --player-backend external to use it.";

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub root: Option<PathBuf>,
    pub ui_scale: f32,
    pub listen: SocketAddr,
    pub player: PlayerConfig,
    pub window: WindowConfig,
    pub yt_dlp: ExtractorConfig,
    pub controls: ControllerConfig,
    pub audio: AudioConfig,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            root: None,
            ui_scale: 1.0,
            listen: "0.0.0.0:8765".parse().unwrap(),
            player: PlayerConfig::default(),
            window: WindowConfig::default(),
            yt_dlp: ExtractorConfig::default(),
            controls: ControllerConfig::default(),
            audio: AudioConfig::default(),
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlayerConfig {
    pub backend: PlayerBackend,
    pub libmpv: Option<PathBuf>,
    pub executable: PathBuf,
    pub fullscreen: bool,
    pub args: Vec<String>,
}
impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            backend: PlayerBackend::Embedded,
            libmpv: None,
            executable: "mpv".into(),
            fullscreen: true,
            args: vec![],
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlayerBackend {
    #[default]
    Embedded,
    External,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WindowMode {
    #[default]
    Fullscreen,
    Windowed,
    Maximized,
}
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindowConfig {
    pub mode: WindowMode,
    pub size: [f32; 2],
    pub decorations: bool,
    pub resizable: bool,
}
impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            mode: WindowMode::Fullscreen,
            size: [1280.0, 720.0],
            decorations: true,
            resizable: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControllerButton {
    South,
    East,
    West,
    North,
    Start,
    Select,
    LeftShoulder,
    RightShoulder,
    LeftThumb,
    RightThumb,
}
impl ControllerButton {
    pub const ALL: [Self; 10] = [
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::Start,
        Self::Select,
        Self::LeftShoulder,
        Self::RightShoulder,
        Self::LeftThumb,
        Self::RightThumb,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::South => "South (A)",
            Self::East => "East (B)",
            Self::West => "West (X)",
            Self::North => "North (Y)",
            Self::Start => "Start / Menu",
            Self::Select => "Select / View",
            Self::LeftShoulder => "Left shoulder",
            Self::RightShoulder => "Right shoulder",
            Self::LeftThumb => "Left stick click",
            Self::RightThumb => "Right stick click",
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ControllerConfig {
    // Confirm, Back, Refresh, Menu, Play/pause, Fullscreen, Mute.
    pub buttons: [ControllerButton; 7],
}
impl Default for ControllerConfig {
    fn default() -> Self {
        use ControllerButton::*;
        Self {
            buttons: [South, East, West, Start, North, Select, RightThumb],
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AudioConfig {
    pub volume: f64,
    pub muted: bool,
}
impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            volume: 100.0,
            muted: false,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExtractorConfig {
    pub executable: Option<PathBuf>,
    pub plugin_dirs: Vec<PathBuf>,
    pub cookies_from_browser: Option<String>,
    pub args: Vec<String>,
}

#[derive(Debug)]
pub struct Settings {
    pub config: Config,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
}
impl Settings {
    /// Read the original file so resolved paths and CLI overrides aren't written
    /// back. Only preferences explicitly edited in the settings screen change.
    pub fn save_preferences(
        &self,
        mode: WindowMode,
        scale: f32,
        audio: &AudioConfig,
        controls: &ControllerConfig,
    ) -> Result<(), String> {
        let original = fs::read_to_string(&self.config_path).map_err(|e| e.to_string())?;
        let mut document: toml::Value = toml::from_str(&original).map_err(|e| e.to_string())?;
        let table = document
            .as_table_mut()
            .ok_or("Configuration must be a table")?;
        table.insert("ui_scale".into(), toml::Value::Float(scale as f64));
        let window = table
            .entry("window")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        window
            .as_table_mut()
            .ok_or("window must be a table")?
            .insert(
                "mode".into(),
                toml::Value::try_from(mode).map_err(|e| e.to_string())?,
            );
        table.insert(
            "audio".into(),
            toml::Value::try_from(audio).map_err(|e| e.to_string())?,
        );
        table.insert(
            "controls".into(),
            toml::Value::try_from(controls).map_err(|e| e.to_string())?,
        );
        let text = toml::to_string_pretty(&document).map_err(|e| e.to_string())?;
        // Keep the original annotated file on the first save; TOML serialization
        // preserves values but does not preserve comments or formatting.
        let backup = self.config_path.with_extension("toml.bak");
        if !backup.exists() {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&backup) {
                Ok(mut file) => file
                    .write_all(original.as_bytes())
                    .map_err(|e| e.to_string())?,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        let parent = self
            .config_path
            .parent()
            .ok_or("Config path has no parent")?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        temporary
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        temporary.as_file().sync_all().map_err(|e| e.to_string())?;
        temporary
            .persist(&self.config_path)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn plugin_dir(&self) -> PathBuf {
        self.data_dir.join("yt-dlp/plugins")
    }
    pub fn prepare(&self) -> Result<(), String> {
        // yt-dlp searches plugin-dir children for namespace packages.
        let extractor = self.plugin_dir().join("user/yt_dlp_plugins/extractor");
        fs::create_dir_all(&extractor)
            .map_err(|e| format!("Cannot create {}: {e}", extractor.display()))?;
        for path in &self.config.yt_dlp.plugin_dirs {
            if !path.is_dir() {
                return Err(format!(
                    "yt-dlp plugin directory does not exist: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }
    pub fn extractor_config(&self) -> Result<String, String> {
        let mut args = vec!["--plugin-dirs".to_owned(), "default".to_owned()];
        for path in std::iter::once(self.plugin_dir()).chain(self.config.yt_dlp.plugin_dirs.clone())
        {
            args.push("--plugin-dirs".into());
            args.push(
                path.to_str()
                    .ok_or("yt-dlp plugin paths must be valid Unicode")?
                    .into(),
            );
        }
        if let Some(browser) = &self.config.yt_dlp.cookies_from_browser {
            args.extend(["--cookies-from-browser".into(), browser.clone()]);
        }
        args.extend(self.config.yt_dlp.args.clone());
        // yt-dlp config files use shlex, including on Windows. No shell is executed.
        args.into_iter()
            .map(|arg| {
                if arg.contains(['\n', '\r', '\0']) {
                    return Err("yt-dlp arguments cannot contain newlines or NUL characters".into());
                }
                Ok(format!("'{}'", arg.replace('\'', "'\"'\"'")))
            })
            .collect::<Result<Vec<_>, String>>()
            .map(|args| format!("# coding: utf-8\n{}\n", args.join(" ")))
    }
}

#[derive(Default)]
struct Overrides {
    config: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    root: Option<PathBuf>,
    mpv: Option<PathBuf>,
    libmpv: Option<PathBuf>,
    backend: Option<PlayerBackend>,
    window_mode: Option<WindowMode>,
    yt_dlp: Option<PathBuf>,
    plugin_dirs: Vec<PathBuf>,
    scale: Option<f32>,
    listen: Option<SocketAddr>,
    show_paths: bool,
}
impl Overrides {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut result = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--show-paths" {
                result.show_paths = true;
                continue;
            }
            let value = match arg.to_str() {
                Some(
                    "--config"
                    | "--data-dir"
                    | "--root"
                    | "--mpv"
                    | "--libmpv"
                    | "--player-backend"
                    | "--window-mode"
                    | "--yt-dlp"
                    | "--yt-dlp-plugin-dir"
                    | "--ui-scale"
                    | "--listen",
                ) => args
                    .next()
                    .ok_or_else(|| format!("{} requires a value", arg.to_string_lossy()))?,
                _ => return Err(format!("Unknown option: {}", arg.to_string_lossy())),
            };
            match arg.to_str().unwrap() {
                "--config" => result.config = Some(value.into()),
                "--data-dir" => result.data_dir = Some(value.into()),
                "--root" => result.root = Some(value.into()),
                "--mpv" => result.mpv = Some(value.into()),
                "--libmpv" => result.libmpv = Some(value.into()),
                "--player-backend" => {
                    result.backend = Some(match value.to_str() {
                        Some("embedded") => PlayerBackend::Embedded,
                        Some("external") => PlayerBackend::External,
                        _ => return Err("--player-backend must be embedded or external".into()),
                    })
                }
                "--window-mode" => {
                    result.window_mode = Some(match value.to_str() {
                        Some("fullscreen") => WindowMode::Fullscreen,
                        Some("windowed") => WindowMode::Windowed,
                        Some("maximized") => WindowMode::Maximized,
                        _ => {
                            return Err(
                                "--window-mode must be fullscreen, windowed, or maximized".into()
                            );
                        }
                    })
                }
                "--yt-dlp" => result.yt_dlp = Some(value.into()),
                "--yt-dlp-plugin-dir" => result.plugin_dirs.push(value.into()),
                "--ui-scale" => {
                    result.scale = Some(
                        value
                            .to_str()
                            .and_then(|s| s.parse().ok())
                            .ok_or("--ui-scale requires a number")?,
                    )
                }
                "--listen" => {
                    result.listen = Some(
                        value
                            .to_str()
                            .and_then(|s| s.parse().ok())
                            .ok_or("--listen requires an IP:PORT address")?,
                    )
                }
                _ => unreachable!(),
            }
        }
        Ok(result)
    }
}

pub fn load() -> Result<Option<Settings>, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return Ok(None);
    }
    let overrides = Overrides::parse(args)?;
    let cwd = std::env::current_dir().map_err(|e| format!("Cannot read current directory: {e}"))?;
    let (default_config, default_data) = default_paths();
    let config_path = overrides
        .config
        .as_ref()
        .map(|p| absolute(p, &cwd))
        .or(default_config)
        .ok_or("Cannot locate user configuration directory; pass --config PATH")?;
    let data_dir = overrides
        .data_dir
        .as_ref()
        .map(|p| absolute(p, &cwd))
        .or(default_data)
        .ok_or("Cannot locate user data directory; pass --data-dir PATH")?;
    if overrides.show_paths {
        println!(
            "Config: {}\nData: {}\nExtractors: {}",
            config_path.display(),
            data_dir.display(),
            data_dir
                .join("yt-dlp/plugins/user/yt_dlp_plugins/extractor")
                .display()
        );
        return Ok(None);
    }
    let settings = read_settings(config_path, data_dir, overrides, &cwd)?;
    settings.prepare()?;
    eprintln!("Configuration: {}", settings.config_path.display());
    Ok(Some(settings))
}

fn default_paths() -> (Option<PathBuf>, Option<PathBuf>) {
    let Some(base) = BaseDirs::new() else {
        return (None, None);
    };
    let name = if cfg!(windows) || cfg!(target_os = "macos") {
        "MediaLauncher"
    } else {
        "media-launcher"
    };
    (
        Some(base.config_dir().join(name).join("config.toml")),
        Some(base.data_dir().join(name)),
    )
}
fn absolute(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    }
}
fn executable(path: &Path, base: &Path) -> PathBuf {
    if path.as_os_str().is_empty() {
        return path.to_owned();
    }
    if path.components().count() == 1
        && matches!(
            path.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        path.to_owned()
    } else {
        absolute(path, base)
    }
}
fn read_settings(
    config_path: PathBuf,
    data_dir: PathBuf,
    overrides: Overrides,
    cwd: &Path,
) -> Result<Settings, String> {
    let text = match fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && overrides.config.is_none() => {
            let parent = config_path.parent().ok_or("Config path has no parent")?;
            fs::create_dir_all(parent)
                .map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&config_path) {
                Ok(mut file) => {
                    file.write_all(TEMPLATE.as_bytes())
                        .map_err(|e| format!("Cannot write {}: {e}", config_path.display()))?;
                    TEMPLATE.into()
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    fs::read_to_string(&config_path)
                        .map_err(|e| format!("Cannot read {}: {e}", config_path.display()))?
                }
                Err(e) => return Err(format!("Cannot create {}: {e}", config_path.display())),
            }
        }
        Err(e) => return Err(format!("Cannot read {}: {e}", config_path.display())),
    };
    let mut config: Config = toml::from_str(&text)
        .map_err(|e| format!("Invalid configuration {}: {e}", config_path.display()))?;
    let base = config_path.parent().ok_or("Config path has no parent")?;
    if config
        .root
        .as_ref()
        .is_some_and(|p| p.as_os_str().is_empty())
        || overrides
            .root
            .as_ref()
            .is_some_and(|p| p.as_os_str().is_empty())
        || config
            .yt_dlp
            .plugin_dirs
            .iter()
            .chain(&overrides.plugin_dirs)
            .any(|p| p.as_os_str().is_empty())
    {
        return Err("Root and plugin directories must not be empty".into());
    }
    if !config.audio.volume.is_finite() || !(0.0..=100.0).contains(&config.audio.volume) {
        return Err("audio.volume must be between 0 and 100".into());
    }
    for (index, button) in config.controls.buttons.iter().enumerate() {
        if config.controls.buttons[..index].contains(button) {
            return Err("controls.buttons must contain distinct controller buttons".into());
        }
    }
    config.root = overrides
        .root
        .map(|p| absolute(&p, cwd))
        .or_else(|| config.root.map(|p| absolute(&p, base)));
    config.player.executable = overrides
        .mpv
        .map(|p| executable(&p, cwd))
        .unwrap_or_else(|| executable(&config.player.executable, base));
    if config
        .player
        .libmpv
        .as_ref()
        .is_some_and(|p| p.as_os_str().is_empty())
        || overrides
            .libmpv
            .as_ref()
            .is_some_and(|p| p.as_os_str().is_empty())
    {
        return Err("player.libmpv must not be empty".into());
    }
    config.player.libmpv = overrides
        .libmpv
        .map(|p| absolute(&p, cwd))
        .or_else(|| config.player.libmpv.map(|p| absolute(&p, base)));
    if let Some(backend) = overrides.backend {
        config.player.backend = backend;
    }
    if let Some(mode) = overrides.window_mode {
        config.window.mode = mode;
    }
    if config
        .window
        .size
        .iter()
        .any(|v| !v.is_finite() || !(320.0..=16384.0).contains(v))
    {
        return Err(
            "window.size must contain two finite dimensions between 320 and 16384 logical pixels"
                .into(),
        );
    }
    config.yt_dlp.executable = overrides
        .yt_dlp
        .map(|p| executable(&p, cwd))
        .or_else(|| config.yt_dlp.executable.map(|p| executable(&p, base)));
    // Windows distributions carry their extractor beside the application.
    // Explicit configuration always wins; Linux packages use system discovery.
    #[cfg(windows)]
    if config.yt_dlp.executable.is_none()
        && let Some(directory) = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_owned))
    {
        let extractor = directory.join("yt-dlp.exe");
        if extractor.is_file() {
            config.yt_dlp.executable = Some(extractor);
            let deno = directory.join("deno.exe");
            if deno.is_file()
                && !config
                    .yt_dlp
                    .args
                    .iter()
                    .any(|arg| arg == "--js-runtimes" || arg.starts_with("--js-runtimes="))
            {
                let path = deno
                    .to_str()
                    .ok_or("Bundled Deno path must be valid Unicode")?;
                config
                    .yt_dlp
                    .args
                    .extend(["--js-runtimes".into(), format!("deno:{path}")]);
            }
        }
    }
    config.yt_dlp.plugin_dirs = if overrides.plugin_dirs.is_empty() {
        config
            .yt_dlp
            .plugin_dirs
            .into_iter()
            .map(|p| absolute(&p, base))
            .collect()
    } else {
        overrides
            .plugin_dirs
            .into_iter()
            .map(|p| absolute(&p, cwd))
            .collect()
    };
    if let Some(scale) = overrides.scale {
        config.ui_scale = scale;
    }
    if let Some(listen) = overrides.listen {
        config.listen = listen;
    }
    if !config.ui_scale.is_finite() || !(0.5..=4.0).contains(&config.ui_scale) {
        return Err("UI scale must be between 0.5 and 4.0".into());
    }
    if config
        .root
        .as_ref()
        .is_some_and(|p| p.as_os_str().is_empty())
        || config.player.executable.as_os_str().is_empty()
    {
        return Err("Root and player executable must not be empty".into());
    }
    for arg in &config.player.args {
        if !arg.starts_with("--") || arg == "--" || arg.contains('\0') {
            return Err("player.args must contain mpv options such as --hwdec=auto; media paths are supplied by the launcher".into());
        }
    }
    if let Some(path) = &config.yt_dlp.executable {
        let text = path
            .to_str()
            .ok_or("yt-dlp executable must be valid Unicode")?;
        let separator = if cfg!(windows) { ';' } else { ':' };
        if text.is_empty() || text.contains([separator, '\n', '\r', '\0']) {
            return Err(format!(
                "yt-dlp executable must be a single nonempty path without {separator}"
            ));
        }
    }
    let settings = Settings {
        config,
        config_path,
        data_dir,
    };
    settings.extractor_config()?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(dir: &Path, text: &str, args: &[&str]) -> Result<Settings, String> {
        let path = dir.join("config.toml");
        fs::write(&path, text).unwrap();
        let overrides = Overrides::parse(args.iter().map(OsString::from))?;
        read_settings(path, dir.join("data"), overrides, &dir.join("cwd"))
    }
    #[test]
    fn first_run_creates_defaults_once_and_preserves_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings/config.toml");
        let settings = read_settings(
            path.clone(),
            dir.path().join("data"),
            Overrides::default(),
            dir.path(),
        )
        .unwrap();
        assert!(settings.config.root.is_none());
        settings.prepare().unwrap();
        assert!(
            settings
                .plugin_dir()
                .join("user/yt_dlp_plugins/extractor")
                .is_dir()
        );
        let plugin = settings
            .plugin_dir()
            .join("user/yt_dlp_plugins/extractor/mine.py");
        fs::write(&plugin, "# user customization").unwrap();
        settings.prepare().unwrap();
        assert_eq!(fs::read_to_string(plugin).unwrap(), "# user customization");
        fs::write(&path, "ui_scale = 1.5").unwrap();
        let settings = read_settings(
            path.clone(),
            dir.path().join("data"),
            Overrides::default(),
            dir.path(),
        )
        .unwrap();
        assert_eq!(settings.config.ui_scale, 1.5);
        assert_eq!(fs::read_to_string(path).unwrap(), "ui_scale = 1.5");
    }
    #[test]
    fn explicit_missing_config_and_unknown_keys_report_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        assert!(
            read_settings(
                path.clone(),
                dir.path().join("data"),
                Overrides {
                    config: Some(path.clone()),
                    ..Default::default()
                },
                dir.path()
            )
            .is_err()
        );
        assert!(!path.exists());
        assert!(
            read(dir.path(), "ui_sacle = 2.0", &[])
                .unwrap_err()
                .contains("ui_sacle")
        );
        assert!(read(dir.path(), "[player]\nexecutabel = 'mpv'", &[]).is_err());
        assert!(read(dir.path(), "root = [", &[]).is_err());
    }
    #[test]
    fn cli_overrides_and_relative_paths_have_predictable_bases() {
        let dir = tempfile::tempdir().unwrap();
        let text = "root = 'videos'\nui_scale = 2.0\nlisten = '127.0.0.1:9000'\n[player]\nexecutable = './bin/mpv'\n[yt_dlp]\nexecutable = 'yt-dlp'\nplugin_dirs = ['plugins']";
        let settings = read(dir.path(), text, &[]).unwrap();
        assert_eq!(settings.config.root, Some(dir.path().join("videos")));
        assert_eq!(
            settings.config.player.executable,
            dir.path().join("./bin/mpv")
        );
        assert_eq!(settings.config.yt_dlp.executable, Some("yt-dlp".into()));
        assert_eq!(
            settings.config.yt_dlp.plugin_dirs,
            vec![dir.path().join("plugins")]
        );
        let settings = read(
            dir.path(),
            text,
            &[
                "--root",
                "other",
                "--mpv",
                "mpv-custom",
                "--yt-dlp",
                "./yt-dlp",
                "--ui-scale",
                "1.25",
                "--listen",
                "0.0.0.0:1234",
                "--yt-dlp-plugin-dir",
                "one",
                "--yt-dlp-plugin-dir",
                "two",
            ],
        )
        .unwrap();
        assert_eq!(settings.config.root, Some(dir.path().join("cwd/other")));
        assert_eq!(
            settings.config.player.executable,
            PathBuf::from("mpv-custom")
        );
        assert_eq!(
            settings.config.yt_dlp.executable,
            Some(dir.path().join("cwd/./yt-dlp"))
        );
        assert_eq!(settings.config.ui_scale, 1.25);
        assert_eq!(settings.config.listen.port(), 1234);
        assert_eq!(
            settings.config.yt_dlp.plugin_dirs,
            vec![dir.path().join("cwd/one"), dir.path().join("cwd/two")]
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("config.toml")).unwrap(),
            text
        );
    }
    #[test]
    fn invalid_settings_fail_before_playback() {
        let dir = tempfile::tempdir().unwrap();
        for text in [
            "ui_scale = nan",
            "ui_scale = 4.1",
            "[audio]\nvolume = nan",
            "[audio]\nvolume = 101",
            "[controls]\nbuttons = ['south', 'east', 'west', 'start', 'north', 'select', 'south']",
            "root = ''",
            "listen = 'bad'",
            "[player]\nexecutable = ''",
            "[player]\nargs = ['--', 'movie.mp4']",
            "[yt_dlp]\nexecutable = ''",
            "[yt_dlp]\nargs = ['a\nb']",
        ] {
            assert!(read(dir.path(), text, &[]).is_err(), "accepted {text}");
        }
        assert!(read(dir.path(), "", &["--ui-scale", "NaN"]).is_err());
        let settings = read(dir.path(), "[yt_dlp]\nplugin_dirs = ['missing']", &[]).unwrap();
        assert!(settings.prepare().unwrap_err().contains("missing"));
    }
    #[test]
    fn window_and_backend_settings_are_validated_and_overridden() {
        let dir = tempfile::tempdir().unwrap();
        let settings = read(dir.path(), "", &[]).unwrap();
        assert_eq!(settings.config.player.backend, PlayerBackend::Embedded);
        assert_eq!(settings.config.window.mode, WindowMode::Fullscreen);
        let text = "[window]\nmode = 'windowed'\nsize = [800, 600]\ndecorations = false\nresizable = false\n[player]\nbackend = 'external'\nlibmpv = './mpv-library'";
        let settings = read(
            dir.path(),
            text,
            &[
                "--player-backend",
                "embedded",
                "--window-mode",
                "maximized",
                "--libmpv",
                "other-library",
            ],
        )
        .unwrap();
        assert_eq!(settings.config.player.backend, PlayerBackend::Embedded);
        assert_eq!(settings.config.window.mode, WindowMode::Maximized);
        assert_eq!(settings.config.window.size, [800.0, 600.0]);
        assert!(!settings.config.window.decorations);
        assert!(!settings.config.window.resizable);
        assert_eq!(
            settings.config.player.libmpv,
            Some(dir.path().join("cwd/other-library"))
        );
        for text in [
            "[window]\nmode = 'bad'",
            "[window]\nsize = [nan, 720]",
            "[window]\nsize = [0, 720]",
            "[window]\nsize = [800]",
            "[player]\nbackend = 'bad'",
            "[player]\nlibmpv = ''",
        ] {
            assert!(read(dir.path(), text, &[]).is_err(), "accepted {text}");
        }
        assert!(read(dir.path(), "", &["--window-mode", "bad"]).is_err());
    }
    #[test]
    fn extractor_configuration_supports_repeated_options_and_special_characters() {
        let dir = tempfile::tempdir().unwrap();
        let settings = read(dir.path(), "[yt_dlp]\nplugin_dirs = [\"plugins,雪's\"]\ncookies_from_browser = 'firefox:My Profile'\nargs = ['--config-locations', 'one.conf', '--config-locations', 'two.conf']", &[]).unwrap();
        let config = settings.extractor_config().unwrap();
        assert_eq!(config.matches("'--plugin-dirs'").count(), 3);
        assert!(config.contains("雪'\"'\"'s"));
        assert!(config.contains("'--cookies-from-browser' 'firefox:My Profile'"));
        assert_eq!(config.matches("'--config-locations'").count(), 2);
    }
    #[test]
    fn preference_save_preserves_original_paths_and_advanced_values() {
        let dir = tempfile::tempdir().unwrap();
        let original = "# keep an annotated backup\nroot = 'videos'\n[window]\nsize = [900, 600]\n[yt_dlp]\nexecutable = './custom-extractor'\nargs = ['--extractor-args', 'site:flag=yes']";
        let settings = read(
            dir.path(),
            original,
            &["--root", "other-videos", "--yt-dlp", "override"],
        )
        .unwrap();
        settings
            .save_preferences(
                WindowMode::Windowed,
                1.2,
                &AudioConfig {
                    volume: 35.0,
                    muted: true,
                },
                &ControllerConfig::default(),
            )
            .unwrap();
        let saved: Config =
            toml::from_str(&fs::read_to_string(&settings.config_path).unwrap()).unwrap();
        assert_eq!(saved.root, Some("videos".into()));
        assert_eq!(saved.yt_dlp.executable, Some("./custom-extractor".into()));
        assert_eq!(saved.yt_dlp.args, ["--extractor-args", "site:flag=yes"]);
        assert_eq!(saved.window.size, [900.0, 600.0]);
        assert_eq!(saved.window.mode, WindowMode::Windowed);
        assert_eq!(saved.audio.volume, 35.0);
        assert!(saved.audio.muted);
        assert_eq!(
            fs::read_to_string(settings.config_path.with_extension("toml.bak")).unwrap(),
            original
        );
        // Saving a minimal existing config also creates missing preference tables.
        let settings = read(dir.path(), "", &[]).unwrap();
        settings
            .save_preferences(
                WindowMode::Fullscreen,
                1.0,
                &AudioConfig::default(),
                &ControllerConfig::default(),
            )
            .unwrap();
    }
}
