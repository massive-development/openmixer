//! The Recording section: folder, file format and the low-disk rule. Each edit is stored in the
//! `record` preferences section and then pushed to the app through [`RecordHost::apply`], so the
//! next recording uses it. Starting and stopping are not settings: they are the `record.*` ids.

use super::{set_if_changed, SettingsError, SettingsResult};
use crate::preferences::{safe_folder, valid_record_format, Preferences, RecordPrefs};

/// Labels of the formats in `RECORD_FORMATS` order.
pub const FORMAT_LABELS: [&str; 3] = ["16-bit", "24-bit", "32-bit float"];

/// What the Recording section needs from the app.
pub trait RecordHost {
    /// Pushes the stored settings; called after every change.
    fn apply(&mut self, prefs: &RecordPrefs);
}

pub struct RecordSettingsPage<'a> {
    prefs: &'a mut Preferences,
    host: &'a mut dyn RecordHost,
}

impl<'a> RecordSettingsPage<'a> {
    pub fn new(prefs: &'a mut Preferences, host: &'a mut dyn RecordHost) -> Self {
        Self { prefs, host }
    }

    pub fn settings(&self) -> &RecordPrefs {
        &self.prefs.record
    }

    /// An empty folder means the default (`<music dir or home>/OpenMixer Recordings`). A path with a
    /// `..` component or a NUL is refused.
    pub fn set_folder(&mut self, folder: &str) -> SettingsResult<bool> {
        let folder = folder.trim();
        if !safe_folder(folder) {
            return Err(SettingsError::InvalidValue { field: "folder" });
        }
        self.edit(|r| set_if_changed(&mut r.folder, folder.to_owned()))
    }

    /// `pcm16`, `pcm24` or `float32`.
    pub fn set_format(&mut self, format: &str) -> SettingsResult<bool> {
        let format =
            valid_record_format(format).ok_or(SettingsError::InvalidValue { field: "format" })?;
        self.edit(|r| set_if_changed(&mut r.format, format.to_owned()))
    }

    pub fn set_stop_on_low_disk(&mut self, on: bool) -> SettingsResult<bool> {
        self.edit(|r| set_if_changed(&mut r.stop_on_low_disk, on))
    }

    fn edit(&mut self, f: impl FnOnce(&mut RecordPrefs) -> bool) -> SettingsResult<bool> {
        let changed = f(&mut self.prefs.record);
        if changed {
            self.host.apply(&self.prefs.record);
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Host(Vec<RecordPrefs>);
    impl RecordHost for Host {
        fn apply(&mut self, prefs: &RecordPrefs) {
            self.0.push(prefs.clone());
        }
    }

    #[test]
    fn edits_store_and_apply_only_on_change() {
        let mut prefs = Preferences::default();
        let mut host = Host::default();
        let mut page = RecordSettingsPage::new(&mut prefs, &mut host);
        assert!(page.set_format("float32").unwrap());
        assert!(!page.set_format("float32").unwrap());
        assert!(page.set_folder(" /srv/rec ").unwrap());
        assert!(page.set_stop_on_low_disk(false).unwrap());
        assert_eq!(host.0.len(), 3);
        assert_eq!(prefs.record.folder, "/srv/rec");
    }

    #[test]
    fn hostile_values_are_refused_and_change_nothing() {
        let mut prefs = Preferences::default();
        let mut host = Host::default();
        let mut page = RecordSettingsPage::new(&mut prefs, &mut host);
        assert!(page.set_format("mp3").is_err());
        assert!(page.set_folder("/a/../../etc").is_err());
        assert!(page.set_folder("bad\0name").is_err());
        assert!(host.0.is_empty());
        assert_eq!(prefs.record, RecordPrefs::default());
    }
}
