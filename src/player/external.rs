use crate::config::Settings;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    ffi::OsStr,
    fs::OpenOptions,
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub struct External {
    child: Option<Child>,
    log: Option<std::path::PathBuf>,
    extractor_config: Option<tempfile::TempPath>,
}
impl External {
    pub fn active(&self) -> bool {
        self.child.is_some()
    }
    pub fn launch(&mut self, settings: &Settings, media: &OsStr) -> Result<(), String> {
        if self.active() {
            return Err("Playback is already active".into());
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let log_path =
            std::env::temp_dir().join(format!("media-launcher-{}-{stamp}.log", std::process::id()));
        let mut log_options = OpenOptions::new();
        log_options.write(true).create_new(true);
        #[cfg(unix)]
        log_options.mode(0o600);
        let log = log_options
            .open(&log_path)
            .map_err(|e| format!("Could not create playback log: {e}"))?;
        let stderr = log
            .try_clone()
            .map_err(|e| format!("Could not open playback log: {e}"))?;
        eprintln!("mpv log: {}", log_path.display());
        self.log = Some(log_path);
        let mut extractor_file = tempfile::Builder::new()
            .prefix("media-launcher-ytdlp-")
            .suffix(".conf")
            .tempfile()
            .map_err(|e| format!("Could not create yt-dlp configuration: {e}"))?;
        extractor_file
            .write_all(settings.extractor_config()?.as_bytes())
            .map_err(|e| format!("Could not write yt-dlp configuration: {e}"))?;
        let extractor_path = extractor_file.into_temp_path();
        self.child = Some(
            command(settings, &extractor_path, media)?
                .stdin(Stdio::null())
                .stdout(Stdio::from(log))
                .stderr(Stdio::from(stderr))
                .spawn()
                .map_err(|e| {
                    format!(
                        "Could not launch {}: {e}",
                        settings.config.player.executable.display()
                    )
                })?,
        );
        self.extractor_config = Some(extractor_path);
        Ok(())
    }
    pub fn poll(&mut self) -> Option<Result<(), String>> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) => {
                self.child = None;
                self.extractor_config = None;
                Some(if status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "mpv exited with {status}. Log: {}",
                        self.log.as_ref().unwrap().display()
                    ))
                })
            }
            // Retain the handle: never enable a second player while its status is unknown.
            Err(e) => {
                eprintln!("Could not check mpv status: {e}");
                None
            }
        }
    }
}
// The -append variants accept one key=value pair without splitting its value
// on commas. Pass it as one native process argument, without shell quoting.
fn command(settings: &Settings, extractor_config: &Path, media: &OsStr) -> Result<Command, String> {
    let mut command = Command::new(&settings.config.player.executable);
    #[cfg(target_os = "linux")]
    command.env_remove("LD_PRELOAD");
    command.arg(if settings.config.player.fullscreen {
        "--fullscreen"
    } else {
        "--no-fullscreen"
    });
    command.args(&settings.config.player.args);
    if let Some(executable) = &settings.config.yt_dlp.executable {
        let executable = executable
            .to_str()
            .ok_or("yt-dlp executable must be valid Unicode")?;
        command.arg(format!(
            "--script-opts-append=ytdl_hook-ytdl_path={}",
            executable
        ));
    }
    let path = extractor_config
        .to_str()
        .ok_or("Temporary directory path must be valid Unicode for yt-dlp")?;
    command.arg(format!(
        "--ytdl-raw-options-append=config-locations={}",
        path
    ));
    command.arg("--").arg(media);
    Ok(command)
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, Instant},
    };
    fn executable(name: &str) -> std::path::PathBuf {
        std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join(name))
            .find(|path| path.is_file())
            .unwrap_or_else(|| panic!("Test requires {name} in PATH"))
    }
    #[test]
    fn literal_arguments_single_child_and_exit_status() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("player");
        fs::write(&script, format!("#!{}\n[ \"$1\" = '--fullscreen' ] || exit 21\n[ \"$3\" = '--' ] || exit 22\n[ -f \"$4\" ] || exit 23\nexit 7\n", executable("sh").display())).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let media = dir.path().join("雪 ' ; $(false).MP4");
        fs::write(&media, b"").unwrap();
        let mut settings = Settings {
            config: crate::config::Config::default(),
            config_path: dir.path().join("config.toml"),
            data_dir: dir.path().join("data"),
        };
        settings.prepare().unwrap();
        let mut player = External::default();
        settings.config.player.executable = dir.path().join("nonexistent-mpv");
        assert!(player.launch(&settings, media.as_os_str()).is_err());
        assert!(!player.active());
        settings.config.player.executable = script.clone();
        player.launch(&settings, media.as_os_str()).unwrap();
        assert!(player.launch(&settings, media.as_os_str()).is_err());
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = player.poll() {
                assert!(result.unwrap_err().contains('7'));
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!player.active());
        settings.config.player.executable = executable("true");
        player.launch(&settings, media.as_os_str()).unwrap();
        loop {
            if let Some(result) = player.poll() {
                assert!(result.is_ok());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::config::Config;
    use std::{
        fs,
        io::Read,
        time::{Duration, Instant},
    };

    #[test]
    #[ignore = "requires installed mpv and yt-dlp; runs headless playback"]
    fn mpv_loads_plugins_from_user_and_extra_directories() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings {
            config: Config::default(),
            config_path: dir.path().join("config.toml"),
            data_dir: dir.path().join("data,雪's"),
        };
        settings.config.player.args = vec![
            "--no-config".into(),
            "--vo=null".into(),
            "--ao=null".into(),
            "--end=0.01".into(),
            "--script-opts-append=ytdl_hook-try_ytdl_first=yes".into(),
        ];
        let executable_name = if cfg!(windows) {
            "yt-dlp.exe"
        } else {
            "yt-dlp"
        };
        let installed = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|p| p.join(executable_name))
            .find(|p| p.is_file())
            .expect("yt-dlp must be installed on PATH");
        let custom_executable = dir.path().join(format!("custom,雪's-{executable_name}"));
        fs::copy(installed, &custom_executable).unwrap();
        settings.config.yt_dlp.executable = Some(custom_executable);
        let extra = dir.path().join("extra,雪's");
        fs::create_dir_all(extra.join("bundle/yt_dlp_plugins/extractor")).unwrap();
        settings.config.yt_dlp.plugin_dirs.push(extra.clone());
        settings.prepare().unwrap();
        // 0.1 seconds of mono 8 kHz PCM silence, with a standard WAV header.
        let mut wav = b"RIFF".to_vec();
        wav.extend(1636_u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(8000_u32.to_le_bytes());
        wav.extend(16000_u32.to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(1600_u32.to_le_bytes());
        wav.resize(1644, 0);
        let media = dir.path().join("silence.wav");
        fs::write(&media, &wav).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let media_url = format!("http://{}/silence.wav", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let server = std::thread::spawn(move || {
            while !stopped.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut request = [0; 4096];
                        let _ = stream.read(&mut request);
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            wav.len()
                        );
                        let _ = stream.write_all(&wav);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(e) => panic!("test server: {e}"),
                }
            }
        });
        for (name, plugin_dir) in [
            ("User", settings.plugin_dir().join("user")),
            ("Extra", extra.join("bundle")),
        ] {
            let marker = dir.path().join(format!("{name}.loaded"));
            let source = format!(
                r#"from pathlib import Path
from yt_dlp.extractor.common import InfoExtractor
class StageTwo{name}IE(InfoExtractor):
    _VALID_URL = r'https://media-launcher.invalid/{name}/(?P<id>test)'
    def _real_extract(self, url):
        Path(r"{}").write_text("loaded")
        return {{'id': 'test', 'title': 'Plugin test', 'url': r"{media_url}", 'ext': 'wav'}}
"#,
                marker.display()
            );
            fs::write(
                plugin_dir.join(format!(
                    "yt_dlp_plugins/extractor/stage_two_{}.py",
                    name.to_lowercase()
                )),
                source,
            )
            .unwrap();
            let mut player = External::default();
            player
                .launch(
                    &settings,
                    OsStr::new(&format!("https://media-launcher.invalid/{name}/test")),
                )
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                if let Some(result) = player.poll() {
                    result.unwrap_or_else(|e| {
                        panic!(
                            "{e}\n{}",
                            fs::read_to_string(player.log.as_ref().unwrap()).unwrap()
                        )
                    });
                    break;
                }
                assert!(Instant::now() < deadline, "mpv did not exit");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(marker.is_file(), "{name} extractor was not invoked");
            assert!(player.extractor_config.is_none());
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        server.join().unwrap();
    }
}
