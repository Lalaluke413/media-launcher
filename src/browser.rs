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

pub const DIRECTORY_NAME_FIT_RATIO: f32 = 0.80;
pub const MIN_COLUMN_WIDTH: f32 = 240.0;
pub const MAX_COLUMN_WIDTH: f32 = 480.0;
pub const COLUMN_PADDING_X: f32 = 18.0;

pub fn column_width(entries: &[Entry], mut measure: impl FnMut(&str) -> f32) -> f32 {
    let mut widths: Vec<_> = entries
        .iter()
        .filter(|e| e.directory)
        .map(|e| measure(&e.name.to_string_lossy()))
        .collect();
    if widths.is_empty() {
        return MIN_COLUMN_WIDTH;
    }
    widths.sort_by(f32::total_cmp);
    let index = (widths.len() as f32 * DIRECTORY_NAME_FIT_RATIO).ceil() as usize - 1;
    (widths[index] + 2.0 * COLUMN_PADDING_X).clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH)
}

/// Camera targets always coincide with a column boundary, including narrow viewports.
pub fn viewport_offset(widths: &[f32], viewport: f32) -> f32 {
    let mut remaining: f32 = widths.iter().sum();
    let mut hidden = 0.0;
    for width in widths.iter().take(widths.len().saturating_sub(1)) {
        if remaining <= viewport {
            break;
        }
        remaining -= width;
        hidden += width;
    }
    -hidden
}

pub struct Column {
    pub directory: PathBuf,
    pub entries: Vec<Entry>,
    pub view: View,
    pub width: f32,
    pub width_pending: bool,
}

pub struct Browser {
    pub configured_root: Option<PathBuf>,
    pub root: Option<PathBuf>,
    pub columns: Vec<Column>,
    pub error: Option<String>,
    history: HashMap<PathBuf, View>,
}

impl Browser {
    pub fn new(root: Option<PathBuf>) -> Self {
        let mut browser = Self {
            configured_root: root,
            root: None,
            columns: vec![],
            error: None,
            history: HashMap::new(),
        };
        browser.refresh();
        browser
    }
    pub fn current(&self) -> Option<&Column> {
        self.columns.last()
    }
    pub fn selected(&self) -> usize {
        self.current().map_or(0, |c| c.view.selected)
    }
    pub fn selected_entry(&self) -> Option<&Entry> {
        self.current().and_then(|c| c.entries.get(c.view.selected))
    }
    pub fn refresh(&mut self) {
        let Some(configured) = &self.configured_root else {
            return;
        };
        if self.root.is_none() {
            match configured.canonicalize() {
                Ok(root) if root.is_dir() => {
                    self.root = Some(root.clone());
                    self.columns.push(Column {
                        directory: root,
                        entries: vec![],
                        view: View::default(),
                        width: MIN_COLUMN_WIDTH,
                        width_pending: true,
                    });
                }
                Ok(_) => {
                    self.error = Some("Library root is not a directory. R to retry.".into());
                    return;
                }
                Err(e) => {
                    self.error = Some(format!("Library root unavailable: {e}. R to retry."));
                    return;
                }
            }
        }
        let column = self.columns.last_mut().unwrap();
        match scan(self.root.as_ref().unwrap(), &column.directory) {
            Ok(entries) => {
                column.view.selected = column
                    .view
                    .path
                    .as_ref()
                    .and_then(|path| entries.iter().position(|e| &e.path == path))
                    .unwrap_or(column.view.selected)
                    .min(entries.len().saturating_sub(1));
                column.view.path = entries.get(column.view.selected).map(|e| e.path.clone());
                column.entries = entries;
                column.width_pending = true;
                self.error = None;
            }
            Err(e) => {
                column.entries.clear();
                column.view.path = None;
                column.width_pending = true;
                self.error = Some(e);
            }
        }
    }
    pub fn select(&mut self, selected: usize) {
        if let Some(c) = self.columns.last_mut() {
            c.view.selected = selected.min(c.entries.len().saturating_sub(1));
            c.view.path = c.entries.get(c.view.selected).map(|e| e.path.clone());
        }
    }
    pub fn navigate(&mut self, directory: PathBuf) {
        let Some(c) = self.current() else {
            return;
        };
        let Some(index) = c
            .entries
            .iter()
            .position(|e| e.directory && e.path == directory)
        else {
            return;
        };
        self.select(index);
        self.columns.push(Column {
            view: self.history.get(&directory).cloned().unwrap_or_default(),
            directory,
            entries: vec![],
            width: MIN_COLUMN_WIDTH,
            width_pending: true,
        });
        self.refresh();
    }
    pub fn back(&mut self) {
        if self.columns.len() <= 1 {
            return;
        }
        let c = self.columns.pop().unwrap();
        self.history.insert(c.directory, c.view);
        self.refresh();
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
    #[cfg(unix)]
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    #[test]
    fn directory_percentile_and_clamps_ignore_files() {
        let entries: Vec<_> = (0..10)
            .map(|n| Entry {
                path: PathBuf::from(n.to_string()),
                name: n.to_string().into(),
                directory: true,
            })
            .collect();
        assert_eq!(
            column_width(&entries, |name| 100.0 + name.parse::<f32>().unwrap() * 20.0),
            276.0
        );
        assert_eq!(column_width(&entries, |_| 1.0), MIN_COLUMN_WIDTH);
        assert_eq!(column_width(&entries, |_| 1000.0), MAX_COLUMN_WIDTH);
        let file = Entry {
            path: "video.mkv".into(),
            name: "video.mkv".into(),
            directory: false,
        };
        let mut mixed = entries.clone();
        mixed.push(file.clone());
        assert_eq!(
            column_width(&mixed, |_| 300.0),
            column_width(&entries, |_| 300.0)
        );
        assert_eq!(
            column_width(&[file], |_| panic!("files must not be measured")),
            MIN_COLUMN_WIDTH
        );
        assert_eq!(column_width(&[], |_| panic!("empty")), MIN_COLUMN_WIDTH);
    }
    #[test]
    fn viewport_targets_whole_column_boundaries() {
        let widths = [240.0, 310.0, 400.0, 260.0];
        assert_eq!(viewport_offset(&widths, 1300.0), 0.0);
        assert_eq!(viewport_offset(&widths, 1000.0), -240.0);
        assert_eq!(viewport_offset(&widths, 700.0), -550.0);
        assert_eq!(viewport_offset(&widths, 100.0), -950.0);
        assert_eq!(viewport_offset(&widths[..3], 700.0), -550.0);
        assert_eq!(viewport_offset(&widths[..2], 700.0), 0.0);
        assert_eq!(viewport_offset(&[], 700.0), 0.0);
    }
    #[test]
    fn columns_scan_on_focus_and_retain_ancestors() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("child")).unwrap();
        let mut b = Browser::new(Some(temp.path().to_owned()));
        b.columns[0].width = 321.0;
        b.columns[0].width_pending = false;
        fs::write(temp.path().join("new.mkv"), b"").unwrap();
        b.select(0);
        assert_eq!(b.columns[0].entries.len(), 1);
        assert_eq!(b.columns[0].width, 321.0);
        b.navigate(temp.path().canonicalize().unwrap().join("child"));
        assert_eq!(b.columns.len(), 2);
        assert!(b.current().unwrap().entries.is_empty());
        assert!(b.error.is_none());
        assert_eq!(b.columns[0].width, 321.0);
        assert_eq!(b.columns[0].entries.len(), 1);
        b.back();
        assert_eq!(b.columns.len(), 1);
        assert_eq!(b.current().unwrap().entries.len(), 2);
        assert_eq!(b.selected_entry().unwrap().name, "child");
        b.back();
        assert_eq!(b.columns.len(), 1);
    }
    #[test]
    fn url_only_mode_does_not_browse_the_working_directory() {
        let mut browser = Browser::new(None);
        browser.refresh();
        browser.back();
        assert!(browser.root.is_none());
        assert!(browser.columns.is_empty());
        assert!(browser.error.is_none());
    }
    #[test]
    fn discovery_is_literal_sorted_and_confined() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        for name in [
            "z.MP4",
            "A.mkv",
            "b.mkv",
            "雪 ' $().m2ts",
            "video.ts",
            ".hidden.mp4",
            "notes.txt",
        ] {
            fs::write(root.join(name), b"").unwrap();
        }
        fs::create_dir(root.join("folder")).unwrap();
        fs::create_dir(root.join(".secret")).unwrap();
        let entries = scan(&root, &root).unwrap();
        assert_eq!(entries.len(), 6);
        assert_eq!(entries[0].name, "folder");
        assert_eq!(entries[1].name, "A.mkv");
        assert_eq!(entries[2].name, "b.mkv");
        let outside = tempfile::tempdir().unwrap();
        assert!(scan(&root, outside.path()).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn native_names_and_symlinks_are_confined() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        fs::write(root.join("z.MP4"), b"").unwrap();
        symlink(root.join("z.MP4"), root.join("alias.mp4")).unwrap();
        symlink("/", root.join("outside")).unwrap();
        symlink(root.join("missing"), root.join("broken.mp4")).unwrap();
        let invalid = OsString::from_vec(b"native\xff.mp4".to_vec());
        fs::write(root.join(&invalid), b"").unwrap();
        let entries = scan(&root, &root).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|e| e.path == root.join(&invalid)));
        assert!(entries.iter().any(|e| e.name == "alias.mp4"));
    }
    #[test]
    fn every_extension_is_visible() {
        let temp = tempfile::tempdir().unwrap();
        for ext in ["mp4", "MKV", "m4v", "WEBM", "avi", "MOV", "ts", "M2TS"] {
            fs::write(temp.path().join(format!("video.{ext}")), b"").unwrap();
        }
        let mut browser = Browser::new(Some(temp.path().to_owned()));
        assert_eq!(browser.current().unwrap().entries.len(), 8);
        browser.select(usize::MAX);
        assert_eq!(browser.selected(), 7);
    }
    #[cfg(unix)]
    #[test]
    fn unreadable_directory_is_recoverable() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let mut browser = Browser::new(Some(temp.path().to_owned()));
        let denied = temp.path().join("denied");
        fs::create_dir(&denied).unwrap();
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();
        // Root bypasses Unix access checks; exercise denial when the OS enforces it.
        if fs::read_dir(&denied).is_err() {
            browser.refresh();
            browser.navigate(denied.clone());
            assert!(browser.error.is_some());
            browser.back(); // leave unavailable directory
            assert_eq!(
                browser.current().unwrap().directory,
                temp.path().canonicalize().unwrap()
            );
        }
        fs::set_permissions(&denied, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlink_retargeted_after_listing_cannot_launch_outside_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(root.path().join("inside.mp4"), b"").unwrap();
        fs::write(outside.path().join("outside.mp4"), b"").unwrap();
        let link = root.path().join("alias.mp4");
        symlink(root.path().join("inside.mp4"), &link).unwrap();
        let browser = Browser::new(Some(root.path().to_owned()));
        let entry = browser
            .current()
            .unwrap()
            .entries
            .iter()
            .find(|e| e.path == link)
            .unwrap();
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
        let mut b = Browser::new(Some(temp.path().to_owned()));
        b.select(0);
        b.columns.last_mut().unwrap().view.scroll = 120.0;
        let root = b.current().unwrap().directory.clone();
        b.navigate(root.join("folder"));
        b.back();
        assert_eq!(b.selected(), 0);
        assert_eq!(b.current().unwrap().view.scroll, 120.0);
        b.select(2);
        fs::remove_file(root.join("a.mp4")).unwrap();
        b.refresh();
        assert_eq!(b.selected(), 1);
        fs::remove_file(root.join("b.mp4")).unwrap();
        b.refresh();
        assert_eq!(b.current().unwrap().entries[b.selected()].name, "c.mp4");
        b.back();
        assert_eq!(b.current().unwrap().directory, root);
    }
    #[test]
    fn unavailable_root_can_be_retried_and_removed_media_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("later");
        let mut b = Browser::new(Some(root.clone()));
        assert!(b.error.is_some());
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a.mp4"), b"").unwrap();
        b.refresh();
        assert!(b.error.is_none());
        fs::remove_file(root.join("a.mp4")).unwrap();
        assert!(b.media_path(&b.current().unwrap().entries[0]).is_err());
    }
}
