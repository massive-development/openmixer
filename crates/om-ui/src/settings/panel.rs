//! Tab bookkeeping for the settings overlay: the standard pages and the plugin-added tabs.

/// The standard tabs, in order. Audio is absent when the app has no audio model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Audio,
    Midi,
    Library,
    Appearance,
    Plugins,
}

impl PageKind {
    pub fn title(self) -> &'static str {
        match self {
            PageKind::Audio => "Audio",
            PageKind::Midi => "MIDI",
            PageKind::Library => "Library",
            PageKind::Appearance => "Appearance",
            PageKind::Plugins => "Plugins",
        }
    }
}

/// Tab bookkeeping for the overlay: which tabs exist, which one is shown, and what Escape and the
/// Close button do. Pages added by plugins are kept by title after the standard ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsPanel {
    tabs: Vec<String>,
    standard: Vec<PageKind>,
    current: usize,
}

impl SettingsPanel {
    pub fn new(has_audio: bool) -> Self {
        let standard: Vec<PageKind> = [
            PageKind::Audio,
            PageKind::Midi,
            PageKind::Library,
            PageKind::Appearance,
            PageKind::Plugins,
        ]
        .into_iter()
        .filter(|k| has_audio || *k != PageKind::Audio)
        .collect();
        Self {
            tabs: standard.iter().map(|k| k.title().to_owned()).collect(),
            standard,
            current: 0,
        }
    }

    pub fn tabs(&self) -> &[String] {
        &self.tabs
    }

    pub fn current(&self) -> usize {
        self.current
    }

    /// The standard page behind tab `index`; `None` for a plugin-added tab.
    pub fn page_kind(&self, index: usize) -> Option<PageKind> {
        self.standard.get(index).copied()
    }

    /// Shows one tab at a time. Out-of-range indices are ignored.
    pub fn show(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.current = index;
        }
    }

    /// Adds a plugin page as a new tab and returns its index.
    pub fn add_page(&mut self, title: &str) -> usize {
        self.tabs.push(title.to_owned());
        self.tabs.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_omits_the_audio_tab_without_a_model_and_shows_one_page() {
        let mut with = SettingsPanel::new(true);
        assert_eq!(
            with.tabs(),
            &["Audio", "MIDI", "Library", "Appearance", "Plugins"]
        );
        let mut without = SettingsPanel::new(false);
        assert_eq!(
            without.tabs(),
            &["MIDI", "Library", "Appearance", "Plugins"]
        );
        assert_eq!(without.page_kind(0), Some(PageKind::Midi));
        without.show(2);
        assert_eq!(without.current(), 2);
        without.show(99);
        assert_eq!(without.current(), 2, "out of range is ignored");
        let plugin_tab = with.add_page("Extra");
        assert_eq!(plugin_tab, 5);
        assert_eq!(with.page_kind(plugin_tab), None);
    }

    #[test]
    fn the_panel_adds_plugin_pages_after_the_standard_ones() {
        let mut panel = SettingsPanel::new(true);
        let index = panel.add_page("Example");
        assert_eq!(index, 5);
        assert_eq!(panel.tabs().last().map(String::as_str), Some("Example"));
        assert_eq!(
            panel.page_kind(index),
            None,
            "plugin tabs have no standard page"
        );
        panel.show(index);
        assert_eq!(panel.current(), index);
        panel.show(99);
        assert_eq!(panel.current(), index, "out-of-range tabs are ignored");
    }
}
