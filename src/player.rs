use std::{
    ffi::OsStr,
    fs::OpenOptions,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub struct Player {
    child: Option<Child>,
    log: Option<std::path::PathBuf>,
}
impl Player {
    pub fn active(&self) -> bool {
        self.child.is_some()
    }
    pub fn launch(&mut self, executable: &OsStr, path: &Path) -> Result<(), String> {
        if self.active() {
            return Err("Playback is already active".into());
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let log_path =
            std::env::temp_dir().join(format!("media-launcher-{}-{stamp}.log", std::process::id()));
        let log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&log_path)
            .map_err(|e| format!("Could not create playback log: {e}"))?;
        let stderr = log
            .try_clone()
            .map_err(|e| format!("Could not open playback log: {e}"))?;
        eprintln!("mpv log: {}", log_path.display());
        self.log = Some(log_path);
        self.child = Some(
            Command::new(executable)
                .arg("--fullscreen")
                .arg("--")
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::from(log))
                .stderr(Stdio::from(stderr))
                .spawn()
                .map_err(|e| format!("Could not launch {}: {e}", executable.to_string_lossy()))?,
        );
        Ok(())
    }
    pub fn poll(&mut self) -> Option<Result<(), String>> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(None) => None,
            Ok(Some(status)) => {
                self.child = None;
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
#[cfg(test)]
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
        fs::write(&script, format!("#!{}\n[ \"$1\" = '--fullscreen' ] || exit 21\n[ \"$2\" = '--' ] || exit 22\n[ -f \"$3\" ] || exit 23\nexit 7\n", executable("sh").display())).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let media = dir.path().join("雪 ' ; $(false).MP4");
        fs::write(&media, b"").unwrap();
        let mut player = Player::default();
        assert!(
            player
                .launch(OsStr::new("/nonexistent/mpv"), &media)
                .is_err()
        );
        assert!(!player.active());
        player.launch(script.as_os_str(), &media).unwrap();
        assert!(player.launch(script.as_os_str(), &media).is_err());
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
        player
            .launch(executable("true").as_os_str(), &media)
            .unwrap();
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
