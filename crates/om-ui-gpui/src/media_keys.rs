//! OS media controls (MPRIS on Linux, the system media transport controls on Windows, the remote
//! command centre on macOS) through `souvlaki`: the keyboard's play/pause key and the desktop's
//! media widget drive the lead deck, and the widget shows the track that deck has loaded.
//!
//! The OS calls back on a thread of its own, so the callback only forwards the event into a
//! channel; the window thread drains it once a frame (`Shell::media_step`) and talks to the
//! engine, as every other input does. Starting is fail-soft: with no session bus (a container, a
//! bare window manager) the app runs without media keys and says why on stderr.

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};
use std::ffi::c_void;
use std::sync::mpsc::{channel, Receiver, Sender};

/// What the lead deck should do for a media-key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    Play,
    Pause,
    PlayPause,
}

/// The command a media event means, or `None` for the ones a DJ deck has no use for (next and
/// previous track would load without asking, seeking is the platter's job).
pub fn command_for(event: &MediaControlEvent) -> Option<MediaCommand> {
    match event {
        MediaControlEvent::Play => Some(MediaCommand::Play),
        MediaControlEvent::Pause | MediaControlEvent::Stop => Some(MediaCommand::Pause),
        MediaControlEvent::Toggle => Some(MediaCommand::PlayPause),
        _ => None,
    }
}

/// What the OS widget is showing, so an unchanged frame sends nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shown {
    title: String,
    artist: String,
    playing: bool,
}

/// The OS media session: the events it delivers and the state it was last given.
pub struct MediaSession {
    /// `None` for a session with no OS behind it (tests).
    controls: Option<MediaControls>,
    events: Receiver<MediaControlEvent>,
    shown: Option<Shown>,
}

impl MediaSession {
    /// Registers with the OS. `hwnd` is the window handle Windows needs (`None` elsewhere).
    /// Returns `None`, after saying why on stderr, when the OS refuses.
    pub fn start(hwnd: Option<*mut c_void>) -> Option<Self> {
        let (sender, events) = channel();
        let config = PlatformConfig {
            display_name: "OpenMixer",
            dbus_name: "openmixer",
            hwnd,
        };
        let attach = |sender: Sender<MediaControlEvent>| -> Result<MediaControls, String> {
            let mut controls = MediaControls::new(config).map_err(|e| format!("{e:?}"))?;
            controls
                .attach(move |event| {
                    // The window is gone when the receiver is: nothing to tell.
                    let _ = sender.send(event);
                })
                .map_err(|e| format!("{e:?}"))?;
            Ok(controls)
        };
        match attach(sender) {
            Ok(controls) => Some(Self {
                controls: Some(controls),
                events,
                shown: None,
            }),
            Err(reason) => {
                eprintln!("openmixer: media keys are off: {reason}");
                None
            }
        }
    }

    /// A session with no OS behind it, and the sender that stands in for the OS.
    pub fn detached() -> (Self, Sender<MediaControlEvent>) {
        let (sender, events) = channel();
        (
            Self {
                controls: None,
                events,
                shown: None,
            },
            sender,
        )
    }

    /// The commands the OS has sent since the last call, in order.
    pub fn drain(&mut self) -> Vec<MediaCommand> {
        self.events
            .try_iter()
            .filter_map(|e| command_for(&e))
            .collect()
    }

    /// Tells the OS widget what is loaded and whether it plays. Sends only on a change.
    pub fn publish(&mut self, title: &str, artist: &str, playing: bool) {
        let now = Shown {
            title: title.to_owned(),
            artist: artist.to_owned(),
            playing,
        };
        if self.shown.as_ref() == Some(&now) {
            return;
        }
        if let Some(controls) = self.controls.as_mut() {
            let metadata = MediaMetadata {
                title: (!title.is_empty()).then_some(title),
                artist: (!artist.is_empty()).then_some(artist),
                ..MediaMetadata::default()
            };
            let playback = if playing {
                MediaPlayback::Playing { progress: None }
            } else {
                MediaPlayback::Paused { progress: None }
            };
            // A failure here costs a stale widget, not the show.
            let _ = controls.set_metadata(metadata);
            let _ = controls.set_playback(playback);
        }
        self.shown = Some(now);
    }

    /// What the widget was last given, as `(title, artist, playing)`.
    pub fn shown(&self) -> Option<(&str, &str, bool)> {
        self.shown
            .as_ref()
            .map(|s| (s.title.as_str(), s.artist.as_str(), s.playing))
    }
}

/// The Win32 window handle souvlaki needs to attach to, from a GPUI window.
#[cfg(windows)]
pub fn window_hwnd(window: &gpui::Window) -> Option<*mut c_void> {
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as *mut c_void),
        _ => None,
    }
}

/// Only Windows needs a window handle to register media controls.
#[cfg(not(windows))]
pub fn window_hwnd(_: &gpui::Window) -> Option<*mut c_void> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_pause_stop_map_and_the_rest_are_ignored() {
        assert_eq!(
            command_for(&MediaControlEvent::Play),
            Some(MediaCommand::Play)
        );
        assert_eq!(
            command_for(&MediaControlEvent::Pause),
            Some(MediaCommand::Pause)
        );
        assert_eq!(
            command_for(&MediaControlEvent::Stop),
            Some(MediaCommand::Pause)
        );
        assert_eq!(
            command_for(&MediaControlEvent::Toggle),
            Some(MediaCommand::PlayPause)
        );
        assert_eq!(command_for(&MediaControlEvent::Next), None);
        assert_eq!(command_for(&MediaControlEvent::Previous), None);
    }

    #[test]
    fn a_detached_session_drains_what_was_sent_in_order() {
        let (mut session, os) = MediaSession::detached();
        os.send(MediaControlEvent::Toggle).unwrap();
        os.send(MediaControlEvent::Next).unwrap();
        os.send(MediaControlEvent::Pause).unwrap();
        assert_eq!(
            session.drain(),
            vec![MediaCommand::PlayPause, MediaCommand::Pause]
        );
        assert!(session.drain().is_empty());
    }

    #[test]
    fn publishing_remembers_the_last_state() {
        let (mut session, _os) = MediaSession::detached();
        session.publish("Title", "Artist", true);
        assert_eq!(session.shown(), Some(("Title", "Artist", true)));
        session.publish("Title", "Artist", false);
        assert_eq!(session.shown(), Some(("Title", "Artist", false)));
    }
}
