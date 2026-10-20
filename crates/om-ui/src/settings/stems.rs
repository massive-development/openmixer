//! The stems section of the library page: the model state and its licence, the cache size and cap,
//! the auto-prepare preference and clearing the cache. Downloads go through the [`StemHost`], and
//! only after [`StemsSettingsPage::accept_and_download`] (the user's acceptance).

use super::{set_if_changed, SettingsResult};
use crate::preferences::Preferences;
use crate::stems::{ModelState, StemHost};

const GIB: u64 = 1024 * 1024 * 1024;

/// Stems page logic over the shared preferences and the app's host.
pub struct StemsSettingsPage<'a> {
    prefs: &'a mut Preferences,
    host: &'a mut dyn StemHost,
}

impl<'a> StemsSettingsPage<'a> {
    pub const MAX_CACHE_GB: i32 = 1000;

    pub fn new(prefs: &'a mut Preferences, host: &'a mut dyn StemHost) -> Self {
        Self { prefs, host }
    }

    pub fn state(&self) -> ModelState {
        self.host.model_state()
    }

    /// The licence name and text the dialog shows, when there is a model to license.
    pub fn licence(&self) -> Option<(String, String)> {
        self.host.licence()
    }

    /// The user accepted the licence: the download starts. Nothing is fetched before this call.
    pub fn accept_and_download(&mut self) -> SettingsResult<()> {
        self.host.accept_and_download()
    }

    pub fn cancel_download(&mut self) {
        self.host.cancel_download();
    }

    pub fn max_cache_gb(&self) -> i32 {
        self.prefs.stems.max_cache_gb
    }

    /// Clamped to `1..=MAX_CACHE_GB`. The host applies the new cap to the next committed job.
    pub fn set_max_cache_gb(&mut self, gb: i32) -> bool {
        let clamped = gb.clamp(1, Self::MAX_CACHE_GB);
        let changed = set_if_changed(&mut self.prefs.stems.max_cache_gb, clamped);
        if changed {
            self.host
                .set_max_cache_bytes(u64::from(clamped as u32) * GIB);
        }
        changed
    }

    pub fn auto_prepare(&self) -> bool {
        self.prefs.stems.auto_prepare
    }

    /// Stored, and pushed to the host so the analysis trigger sees it.
    pub fn set_auto_prepare(&mut self, on: bool) -> bool {
        let changed = set_if_changed(&mut self.prefs.stems.auto_prepare, on);
        if changed {
            self.host.set_auto_prepare(on);
        }
        changed
    }

    /// Bytes the cache holds now.
    pub fn cache_bytes(&self) -> u64 {
        self.host.cache_bytes()
    }

    /// The cap in bytes.
    pub fn cap_bytes(&self) -> u64 {
        u64::from(self.max_cache_gb() as u32) * GIB
    }

    /// `"1.2 GB of 20 GB used"`.
    pub fn usage_text(&self) -> String {
        format!(
            "{} of {} used",
            format_gib(self.cache_bytes()),
            format_gib(self.cap_bytes())
        )
    }

    /// Cancels the running prepare jobs, removes every stem set and index row, and returns how many
    /// sets were removed. The caller confirms first.
    pub fn clear_cache(&mut self) -> SettingsResult<usize> {
        self.host.clear_cache()
    }
}

/// Bytes as gigabytes with one decimal, for the page (`0.0 GB` for an empty cache).
pub fn format_gib(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / GIB as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stems::StemJob;
    use om_core::jobs::JobHandle;

    #[derive(Default)]
    struct Host {
        cap: u64,
        downloads: usize,
        cleared: usize,
        state: Option<ModelState>,
        auto: bool,
    }

    impl StemHost for Host {
        fn model_state(&self) -> ModelState {
            self.state.clone().unwrap_or(ModelState::LicenceNotAccepted)
        }
        fn licence(&self) -> Option<(String, String)> {
            Some(("MIT".into(), "licence text".into()))
        }
        fn accept_and_download(&mut self) -> SettingsResult<()> {
            self.downloads += 1;
            Ok(())
        }
        fn cancel_download(&mut self) {}
        fn separator(&self) -> (String, i32) {
            ("htdemucs".into(), 1)
        }
        fn cache_bytes(&self) -> u64 {
            3 * GIB / 2
        }
        fn clear_cache(&mut self) -> SettingsResult<usize> {
            self.cleared += 1;
            Ok(4)
        }
        fn set_max_cache_bytes(&mut self, bytes: u64) {
            self.cap = bytes;
        }
        fn auto_prepare(&self) -> bool {
            self.auto
        }
        fn set_auto_prepare(&mut self, on: bool) {
            self.auto = on;
        }
        fn submit(&mut self, _job: StemJob) -> SettingsResult<JobHandle> {
            unreachable!("the page never submits")
        }
    }

    #[test]
    fn the_cap_is_clamped_stored_and_pushed_to_the_host() {
        let mut prefs = Preferences::default();
        let mut host = Host::default();
        let mut page = StemsSettingsPage::new(&mut prefs, &mut host);
        assert!(page.set_max_cache_gb(0));
        assert_eq!(page.max_cache_gb(), 1);
        assert!(page.set_max_cache_gb(100_000));
        assert_eq!(page.max_cache_gb(), StemsSettingsPage::MAX_CACHE_GB);
        assert!(!page.set_max_cache_gb(StemsSettingsPage::MAX_CACHE_GB));
        assert_eq!(host.cap, 1000 * GIB);
    }

    #[test]
    fn auto_prepare_is_off_by_default_and_toggles() {
        let mut prefs = Preferences::default();
        let mut host = Host::default();
        {
            let mut page = StemsSettingsPage::new(&mut prefs, &mut host);
            assert!(!page.auto_prepare());
            assert!(page.set_auto_prepare(true));
            assert!(!page.set_auto_prepare(true));
            assert!(page.auto_prepare());
        }
        assert!(host.auto, "the host sees the preference");
    }

    #[test]
    fn usage_reads_bytes_against_the_cap() {
        let mut prefs = Preferences::default();
        let mut host = Host::default();
        let page = StemsSettingsPage::new(&mut prefs, &mut host);
        assert_eq!(page.usage_text(), "1.5 GB of 20.0 GB used");
    }

    #[test]
    fn clearing_and_licence_reach_the_host_without_downloads() {
        let mut prefs = Preferences::default();
        let mut host = Host::default();
        let mut page = StemsSettingsPage::new(&mut prefs, &mut host);
        assert_eq!(page.licence().expect("licence").0, "MIT");
        assert_eq!(page.clear_cache().expect("clear"), 4);
        assert_eq!(host.cleared, 1);
        assert_eq!(host.downloads, 0);
    }
}
