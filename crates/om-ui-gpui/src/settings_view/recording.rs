//! The Recording tab: the folder recordings go to, the file format, the low-disk rule and the
//! status line. The logic is om-ui's `RecordSettingsPage`; this file draws the chips and forwards
//! their clicks to the methods on [`SettingsState`] below. Starting and stopping are not here: they
//! are the header Record button and the `r` key (the `record.*` ids).

use super::chips::*;
use super::*;
use om_ui::preferences::{RecordPrefs, RECORD_FORMATS};
use om_ui::settings::{RecordHost, RecordSettingsPage, RECORD_FORMAT_LABELS};

/// Stands in for the recorder when there is none: settings are stored, nothing is pushed.
struct NoRecord;

impl RecordHost for NoRecord {
    fn apply(&mut self, _: &RecordPrefs) {}
}

pub(super) fn recording_page(
    t: &ThemeTokens,
    settings: &SettingsState,
    cx: &mut Context<Shell>,
) -> Div {
    let r = &settings.prefs.record;
    let folder = if r.folder.is_empty() {
        "Default: OpenMixer Recordings in your music folder".to_owned()
    } else {
        format!("Folder: {}", r.folder)
    };
    let mut column = div().flex().flex_col();
    column = column.child(choice_row(
        t,
        "Folder",
        row_of(vec![
            chip(t, "Change folder", false, cx, |this| {
                this.settings_mut(|s| s.begin_entry(EntryKind::RecordFolder))
            }),
            chip(t, "Use default", r.folder.is_empty(), cx, |this| {
                this.settings_mut(|s| s.record_set_folder(""))
            }),
        ]),
    ));
    column = column.child(note(t, &folder));
    if let Some(field) = entry_field(t, settings, EntryKind::RecordFolder, "folder path", cx) {
        column = column.child(field);
    }

    let formats: Vec<Div> = RECORD_FORMATS
        .iter()
        .zip(RECORD_FORMAT_LABELS)
        .map(|(code, label)| {
            let lit = r.format == *code;
            let code = code.to_string();
            chip(t, label, lit, cx, move |this| {
                this.settings_mut(|s| s.record_set_format(&code))
            })
        })
        .collect();
    column = column.child(choice_row(t, "File format", row_of(formats)));

    let on = r.stop_on_low_disk;
    column = column.child(choice_row(
        t,
        "Stop when the disk is nearly full",
        row_of(vec![chip(t, on_off(on), on, cx, move |this| {
            this.settings_mut(|s| s.record_set_stop_on_low_disk(!on))
        })]),
    ));

    let status = format!(
        "Recordings start and stop with the Record button in the header or the r key. \
         Files are written to the folder above in {}.",
        RECORD_FORMAT_LABELS
            .get(
                RECORD_FORMATS
                    .iter()
                    .position(|f| *f == r.format)
                    .unwrap_or(0)
            )
            .copied()
            .unwrap_or("16-bit")
    );
    column = column.child(note(t, &status));
    column.child(status_line(t, &settings.notice))
}

fn on_off(on: bool) -> &'static str {
    if on {
        "On"
    } else {
        "Off"
    }
}

impl SettingsState {
    /// Runs one page edit on the stored settings. A change is saved and pushed to the recorder
    /// (through the record host); a refused value is shown as the notice.
    fn record_edit(
        &mut self,
        edit: impl FnOnce(&mut RecordSettingsPage<'_>) -> SettingsResult<bool>,
    ) {
        let result = {
            let mut none = NoRecord;
            let host: &mut dyn RecordHost = match self.record.as_deref_mut() {
                Some(host) => host,
                None => &mut none,
            };
            let mut page = RecordSettingsPage::new(&mut self.prefs, host);
            edit(&mut page)
        };
        match result {
            Ok(true) => self.changed(),
            Ok(false) => {}
            Err(error) => self.set_notice(format!("Recording: {error}"), true),
        }
    }

    pub fn record_set_folder(&mut self, folder: &str) {
        self.record_edit(|page| page.set_folder(folder));
    }

    pub fn record_set_format(&mut self, format: &str) {
        self.record_edit(|page| page.set_format(format));
    }

    pub fn record_set_stop_on_low_disk(&mut self, on: bool) {
        self.record_edit(|page| page.set_stop_on_low_disk(on));
    }
}
