use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: OsString,
    pub directory: bool,
}

pub fn scan(root: &Path, directory: &Path) -> Result<Vec<Entry>, String> {
    let canonical = directory
        .canonicalize()
        .map_err(|e| format!("Cannot open {}: {e}", directory.display()))?;
    if !canonical.starts_with(root) {
        return Err("Directory is outside the library root".into());
    }
    let children = fs::read_dir(&canonical)
        .map_err(|e| format!("Cannot read {}: {e}", directory.display()))?;
    let mut entries = Vec::new();
    for child in children {
        let child = match child {
            Ok(child) => child,
            Err(e) => {
                eprintln!("Skipping unreadable entry: {e}");
                continue;
            }
        };
        let name = child.file_name();
        if name.as_encoded_bytes().starts_with(b".") {
            continue;
        }
        let path = directory.join(&name);
        let target = match path.canonicalize() {
            Ok(p) if p.starts_with(root) => p,
            _ => continue,
        };
        let metadata = match fs::metadata(target) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Skipping {}: {e}", path.display());
                continue;
            }
        };
        let directory = metadata.is_dir();
        let video = path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| {
                ["mp4", "mkv", "m4v", "webm", "avi", "mov", "ts", "m2ts"]
                    .iter()
                    .any(|v| ext.eq_ignore_ascii_case(v))
            });
        if directory || (metadata.is_file() && video) {
            entries.push(Entry {
                path,
                name,
                directory,
            });
        }
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| {
                a.name
                    .to_string_lossy()
                    .to_lowercase()
                    .cmp(&b.name.to_string_lossy().to_lowercase())
            })
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

#[derive(Default, Clone)]
pub struct View {
    pub selected: usize,
    pub path: Option<PathBuf>,
    pub scroll: f32,
}

pub struct Browser {
    pub configured_root: PathBuf,
    pub root: Option<PathBuf>,
    pub current: PathBuf,
    pub entries: Vec<Entry>,
    pub view: View,
    pub error: Option<String>,
    history: HashMap<PathBuf, View>,
}

impl Browser {
    pub fn new(root: PathBuf) -> Self {
        let mut browser = Self {
            current: root.clone(),
            configured_root: root,
            root: None,
            entries: vec![],
            view: View::default(),
            error: None,
            history: HashMap::new(),
        };
        browser.refresh();
        browser
    }
    pub fn refresh(&mut self) {
        if self.root.is_none() {
            match self.configured_root.canonicalize() {
                Ok(root) if root.is_dir() => {
                    self.current = root.clone();
                    self.root = Some(root);
                }
                Ok(_) => {
                    self.error = Some("Library root is not a directory. X / R to retry.".into());
                    return;
                }
                Err(e) => {
                    self.error = Some(format!("Library root unavailable: {e}. X / R to retry."));
                    return;
                }
            }
        }
        match scan(self.root.as_ref().unwrap(), &self.current) {
            Ok(entries) => {
                let selected = self
                    .view
                    .path
                    .as_ref()
                    .and_then(|path| entries.iter().position(|e| &e.path == path))
                    .unwrap_or(self.view.selected)
                    .min(entries.len().saturating_sub(1));
                self.entries = entries;
                self.select(selected);
                self.error = None;
            }
            Err(e) => {
                self.entries.clear();
                self.error = Some(e);
            }
        }
    }
    pub fn select(&mut self, selected: usize) {
        self.view.selected = selected.min(self.entries.len().saturating_sub(1));
        self.view.path = self.entries.get(self.view.selected).map(|e| e.path.clone());
    }
    pub fn navigate(&mut self, directory: PathBuf) {
        let Some(root) = &self.root else {
            return;
        };
        if !directory.starts_with(root) {
            return;
        }
        self.history.insert(self.current.clone(), self.view.clone());
        self.current = directory;
        self.view = self.history.get(&self.current).cloned().unwrap_or_default();
        self.refresh();
    }
    pub fn back(&mut self) {
        if self.error.take().is_some() {
            return;
        }
        if self.root.as_ref() == Some(&self.current) {
            return;
        }
        if let Some(parent) = self.current.parent() {
            self.navigate(parent.to_owned());
        }
    }
    pub fn media_path(&self, entry: &Entry) -> Result<PathBuf, String> {
        let path = entry
            .path
            .canonicalize()
            .map_err(|e| format!("File unavailable: {e}"))?;
        if !self
            .root
            .as_ref()
            .is_some_and(|root| path.starts_with(root))
            || !path.is_file()
        {
            return Err("Selected file is unavailable or outside the library root".into());
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    #[test]
    fn discovery_is_literal_sorted_and_confined() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        for name in [
            "z.MP4",
            "A.mkv",
            "a.mkv",
            "雪 ' $().m2ts",
            "video.ts",
            ".hidden.mp4",
            "notes.txt",
        ] {
            fs::write(root.join(name), b"").unwrap();
        }
        fs::create_dir(root.join("folder")).unwrap();
        fs::create_dir(root.join(".secret")).unwrap();
        symlink(root.join("z.MP4"), root.join("alias.mp4")).unwrap();
        symlink("/", root.join("outside")).unwrap();
        symlink(root.join("missing"), root.join("broken.mp4")).unwrap();
        let invalid = OsString::from_vec(b"native\xff.mp4".to_vec());
        fs::write(root.join(&invalid), b"").unwrap();
        let entries = scan(&root, &root).unwrap();
        assert_eq!(entries.len(), 8);
        assert_eq!(entries[0].name, "folder");
        assert_eq!(entries[1].name, "A.mkv");
        assert_eq!(entries[2].name, "a.mkv");
        assert!(entries.iter().any(|e| e.path == root.join(&invalid)));
        assert!(scan(&root, Path::new("/")).is_err());
    }
    #[test]
    fn every_extension_is_visible_and_unreadable_directory_is_recoverable() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        for ext in ["mp4", "MKV", "m4v", "WEBM", "avi", "MOV", "ts", "M2TS"] {
            fs::write(temp.path().join(format!("video.{ext}")), b"").unwrap();
        }
        let mut browser = Browser::new(temp.path().to_owned());
        assert_eq!(browser.entries.len(), 8);
        browser.select(usize::MAX);
        assert_eq!(browser.view.selected, 7);
        let denied = temp.path().join("denied");
        fs::create_dir(&denied).unwrap();
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();
        // Root bypasses Unix access checks; exercise denial when the OS enforces it.
        if fs::read_dir(&denied).is_err() {
            browser.navigate(denied.clone());
            assert!(browser.error.is_some());
            browser.back(); // dismiss error
            browser.back(); // leave unavailable directory
            assert_eq!(browser.current, temp.path().canonicalize().unwrap());
        }
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[test]
    fn symlink_retargeted_after_listing_cannot_launch_outside_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(root.path().join("inside.mp4"), b"").unwrap();
        fs::write(outside.path().join("outside.mp4"), b"").unwrap();
        let link = root.path().join("alias.mp4");
        symlink(root.path().join("inside.mp4"), &link).unwrap();
        let browser = Browser::new(root.path().to_owned());
        let entry = browser.entries.iter().find(|e| e.path == link).unwrap();
        fs::remove_file(&link).unwrap();
        symlink(outside.path().join("outside.mp4"), &link).unwrap();
        assert!(browser.media_path(entry).is_err());
    }
    #[test]
    fn remembers_views_and_refreshes_selection() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("folder")).unwrap();
        for name in ["a.mp4", "b.mp4", "c.mp4"] {
            fs::write(temp.path().join(name), b"").unwrap();
        }
        let mut b = Browser::new(temp.path().to_owned());
        b.select(2);
        b.view.scroll = 120.0;
        let root = b.current.clone();
        b.navigate(root.join("folder"));
        b.back();
        assert_eq!(b.view.selected, 2);
        assert_eq!(b.view.scroll, 120.0);
        fs::remove_file(root.join("a.mp4")).unwrap();
        b.refresh();
        assert_eq!(b.view.selected, 1);
        fs::remove_file(root.join("b.mp4")).unwrap();
        b.refresh();
        assert_eq!(b.entries[b.view.selected].name, "c.mp4");
        b.back();
        assert_eq!(b.current, root);
    }
    #[test]
    fn unavailable_root_can_be_retried_and_removed_media_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("later");
        let mut b = Browser::new(root.clone());
        assert!(b.error.is_some());
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a.mp4"), b"").unwrap();
        b.refresh();
        assert!(b.error.is_none());
        fs::remove_file(root.join("a.mp4")).unwrap();
        assert!(b.media_path(&b.entries[0]).is_err());
    }
}
