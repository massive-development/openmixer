//! Desktop integration (#229): the OS media keys and now-playing widget, native file pickers,
//! the clipboard, and system notifications when a long job ends. All of it runs on the window
//! thread, from `Shell::desktop_step` once a frame or from a button's click.
//!
//! File pickers, the clipboard and system notifications are GPUI's own (`prompt_for_paths`,
//! `write_to_clipboard`, and the component library's system delivery), so they run on GPUI's event
//! loop and need no crate of ours. Only the media controls (`souvlaki`) are separate; see
//! `media_keys`.

use crate::app::Shell;
use crate::debug_panel::play_value;
use crate::media_keys::{MediaCommand, MediaSession};
use crate::settings_view::{EntryKind, SettingsState, TextEntry};
use gpui::{ClipboardItem, Context, PathPromptOptions, Window};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::WindowExt as _;
use om_core::param_id::DeckParam;
use om_ui::binding::DeckBinding;
use std::path::Path;
use std::sync::Arc;

impl EntryKind {
    /// What a file picker for this field asks for: `(directories, prompt)`, or `None` for a field
    /// that is not a path.
    fn picker(self) -> Option<(bool, &'static str)> {
        match self {
            EntryKind::WatchFolder => Some((true, "Add watch folder")),
            EntryKind::RecordFolder => Some((true, "Recording folder")),
            EntryKind::ImportXml => Some((false, "Import iTunes or Rekordbox XML")),
            EntryKind::MapFile | EntryKind::MapImport => Some((false, "Controller map (JSON)")),
            EntryKind::MapExport
            | EntryKind::MapFilter
            | EntryKind::MapField(_)
            | EntryKind::MaxCacheGb => None,
        }
    }

    /// Whether the settings offer a Browse button for this field.
    pub fn can_browse(self) -> bool {
        self.picker().is_some()
    }
}

impl SettingsState {
    /// A path was picked for `kind` in a file dialog: it is applied as if typed and confirmed.
    pub fn apply_pick(&mut self, kind: EntryKind, path: &Path) {
        self.entry = Some(TextEntry {
            kind,
            text: path.to_string_lossy().into_owned(),
        });
        self.submit_entry();
    }
}

impl Shell {
    /// The deck the media keys and the now-playing widget follow: the sync leader's, else the
    /// first playing deck's, else deck 1.
    pub(crate) fn lead_deck(&self) -> usize {
        let leader = self.engine.sync().leader();
        let playing = (0..self.decks.len()).find(|&d| self.engine.deck(d).is_playing());
        leader.or(playing).unwrap_or(0)
    }

    /// Once a frame: the media session, and the announcements of jobs that ended.
    pub(crate) fn desktop_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.options.media_keys && !self.media_tried {
            self.media_tried = true;
            self.media = MediaSession::start(crate::media_keys::window_hwnd(window));
        }
        self.media_step();
        self.announce_finished(window, cx);
    }

    /// Runs the media keys pressed since the last frame on the lead deck, then shows the deck's
    /// track and state in the OS widget.
    pub(crate) fn media_step(&mut self) {
        let Some(commands) = self.media.as_mut().map(MediaSession::drain) else {
            return;
        };
        let deck = self.lead_deck();
        for command in commands {
            let playing = self.engine.deck(deck).is_playing();
            let wanted = match command {
                MediaCommand::Play => true,
                MediaCommand::Pause => false,
                MediaCommand::PlayPause => !playing,
            };
            if wanted != playing {
                let binding =
                    DeckBinding::typed(Arc::clone(&self.engine), Arc::clone(&self.router), deck);
                binding.send(DeckParam::Play, play_value(playing));
            }
        }
        let info = self
            .library
            .as_ref()
            .map(|panel| panel.track_info(deck))
            .unwrap_or_default();
        let playing = self.engine.deck(deck).is_playing();
        if let Some(media) = self.media.as_mut() {
            media.publish(&info.title, &info.artist, playing);
        }
    }

    /// A stem job queue that has just emptied, or a recording that has just stopped, is announced
    /// as a toast and, when the window is in the background, in the desktop's notification area.
    fn announce_finished(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let stemming = self
            .library
            .as_ref()
            .is_some_and(|panel| panel.stems_in_flight() > 0);
        let recorder = self.engine.recorder().snapshot();
        let announcement = match (self.was_stemming, stemming, self.was_recording, recorder) {
            (_, _, true, r) if !r.recording && r.failed => Some(Notification::error(
                "Recording stopped: the disk write failed",
            )),
            (_, _, true, r) if !r.recording => Some(Notification::success("Recording saved")),
            (true, false, _, _) => Some(Notification::success("Stems are ready")),
            _ => None,
        };
        self.was_stemming = stemming;
        self.was_recording = recorder.recording;
        if let Some(note) = announcement {
            window.push_notification(note.title("OpenMixer").in_app_and_system(), cx);
        }
    }

    /// Opens the native file dialog for the settings field `kind`; the pick is applied as typed.
    pub(crate) fn browse(&mut self, kind: EntryKind, cx: &mut Context<Self>) {
        let Some((directories, prompt)) = kind.picker() else {
            return;
        };
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple: false,
            prompt: Some(prompt.into()),
        });
        cx.spawn(async move |this, cx| {
            // A cancelled dialog and a portal that could not open both end here.
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| {
                this.settings_mut(|s| s.apply_pick(kind, &path));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Puts the library's selected tracks on the clipboard, one line each, and says so.
    pub(crate) fn copy_selected_tracks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .library
            .as_mut()
            .map(|panel| panel.selected_summary())
            .unwrap_or_default();
        if text.is_empty() {
            window.push_notification(Notification::info("Select a track to copy"), cx);
            return;
        }
        let count = text.lines().count();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let what = if count == 1 { "track" } else { "tracks" };
        window.push_notification(Notification::success(format!("Copied {count} {what}")), cx);
    }
}
