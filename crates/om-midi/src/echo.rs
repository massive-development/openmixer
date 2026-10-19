//! Echo suppression for controller feedback. A controller whose input and output share a port
//! sends its own LED messages back as input. Feedback records each message it sends to a port;
//! an input message on that port that exactly matches a recorded one within [`ECHO_WINDOW`] is an
//! echo and is dropped. Each record matches at most once, so a real press later is not lost.

use crate::lock;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a sent message can come back as an echo.
pub const ECHO_WINDOW: Duration = Duration::from_millis(100);

#[derive(Default)]
pub struct EchoGuard {
    sent: Mutex<HashMap<String, Vec<(Instant, Vec<u8>)>>>,
}

impl EchoGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a message sent to the output port `port` (named as its input is).
    pub fn note_sent(&self, port: &str, bytes: &[u8], now: Instant) {
        let mut sent = lock(&self.sent);
        let records = sent.entry(port.to_owned()).or_default();
        records.retain(|(at, _)| is_recent(*at, now));
        records.push((now, bytes.to_vec()));
    }

    /// True when `bytes` arriving on the input `port` at `now` is an echo of a recorded send. The
    /// matching record is used up.
    pub fn is_echo(&self, port: &str, bytes: &[u8], now: Instant) -> bool {
        let mut sent = lock(&self.sent);
        let Some(records) = sent.get_mut(port) else {
            return false;
        };
        records.retain(|(at, _)| is_recent(*at, now));
        match records.iter().position(|(_, b)| b == bytes) {
            Some(index) => {
                records.remove(index);
                true
            }
            None => false,
        }
    }
}

fn is_recent(sent_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(sent_at) <= ECHO_WINDOW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_sent_to_a_port_is_an_echo_inside_the_window_only_once() {
        let guard = EchoGuard::new();
        let t0 = Instant::now();
        let on = [0x90, 36, 127];
        guard.note_sent("Pad", &on, t0);

        assert!(
            !guard.is_echo("Other", &on, t0),
            "other ports are not echoes"
        );
        assert!(
            !guard.is_echo("Pad", &[0x90, 37, 127], t0),
            "other bytes are not echoes"
        );
        assert!(
            !guard.is_echo("Pad", &on, t0 + ECHO_WINDOW + Duration::from_millis(1)),
            "after the window the message is a real one"
        );

        guard.note_sent("Pad", &on, t0);
        assert!(guard.is_echo("Pad", &on, t0 + Duration::from_millis(10)));
        assert!(
            !guard.is_echo("Pad", &on, t0 + Duration::from_millis(20)),
            "one send is echoed once"
        );
    }
}
