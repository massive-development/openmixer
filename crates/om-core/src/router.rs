use crate::sync::LockExt;
use crate::ParameterTarget;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, ThreadId};

/// Routes `"prefix.rest"` parameter ids to registered targets; the registry is the extension point.
/// Routing matches raw string prefixes (registry prefixes such as `deck.0` are arbitrary), so the id is
/// not parsed here; [`crate::ParamId`] is the typed view of the same ids.
///
/// Registration and dispatch are serialised by a reentrant gate that is held across the target call,
/// so [`ParameterRouter::unregister_target`] returns only once no call into that target is in flight.
/// A target may route a follow-up id back through the router on the same thread. Never call from the
/// audio thread.
#[derive(Default)]
pub struct ParameterRouter {
    gate: Gate,
    targets: Mutex<BTreeMap<String, Arc<dyn ParameterTarget>>>,
    unknown: AtomicUsize,
}

impl ParameterRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `target` under `prefix`. Re-registering a prefix replaces its target.
    pub fn register_target(&self, prefix: &str, target: Arc<dyn ParameterTarget>) {
        let _gate = self.gate.enter();
        lock(&self.targets).insert(prefix.to_owned(), target);
    }

    /// Removes the target under `prefix`, waiting for any call already routed to it to finish.
    pub fn unregister_target(&self, prefix: &str) {
        let _gate = self.gate.enter();
        lock(&self.targets).remove(prefix);
    }

    /// Number of `set_parameter` calls that were not handled by any target.
    pub fn unknown_count(&self) -> usize {
        self.unknown.load(Ordering::Relaxed)
    }
}

impl ParameterTarget for ParameterRouter {
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        let _gate = self.gate.enter();
        let routed = {
            let targets = lock(&self.targets);
            // Longest registered prefix wins ("deck.0" beats "deck").
            targets
                .iter()
                .filter_map(|(prefix, target)| {
                    let rest = id.strip_prefix(prefix.as_str())?.strip_prefix('.')?;
                    Some((prefix.len(), Arc::clone(target), rest))
                })
                .max_by_key(|(len, _, _)| *len)
                .map(|(_, target, rest)| (target, rest))
        };
        let handled = match routed {
            Some((target, rest)) => target.set_parameter(rest, value),
            None => false,
        };
        if !handled {
            self.unknown.fetch_add(1, Ordering::Relaxed);
        }
        handled
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock_unpoisoned()
}

/// Reentrant exclusion: one owning thread at a time, and the owner may enter again.
#[derive(Default)]
struct Gate {
    state: Mutex<GateState>,
    released: Condvar,
}

#[derive(Default)]
struct GateState {
    owner: Option<ThreadId>,
    depth: usize,
}

struct GateGuard<'a> {
    gate: &'a Gate,
}

impl Gate {
    fn enter(&self) -> GateGuard<'_> {
        let me = thread::current().id();
        let mut state = lock(&self.state);
        loop {
            match state.owner {
                None => {
                    state.owner = Some(me);
                    state.depth = 1;
                    break;
                }
                Some(owner) if owner == me => {
                    state.depth += 1;
                    break;
                }
                Some(_) => {
                    state = self
                        .released
                        .wait(state)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
        GateGuard { gate: self }
    }
}

impl Drop for GateGuard<'_> {
    fn drop(&mut self) {
        let mut state = lock(&self.gate.state);
        state.depth -= 1;
        if state.depth == 0 {
            state.owner = None;
            self.gate.released.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ParameterRouter;
    use crate::ParameterTarget;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Counts calls and records any that arrive after the owner declared it gone.
    #[derive(Default)]
    struct Probe {
        gone: AtomicBool,
        after_gone: AtomicUsize,
        calls: AtomicUsize,
    }

    impl ParameterTarget for Probe {
        fn set_parameter(&self, _id: &str, _value: f64) -> bool {
            if self.gone.load(Ordering::SeqCst) {
                self.after_gone.fetch_add(1, Ordering::SeqCst);
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            true
        }
    }

    /// Records the id it was handed, so routing can be checked.
    #[derive(Default)]
    struct Echo {
        last: std::sync::Mutex<String>,
    }

    impl ParameterTarget for Echo {
        fn set_parameter(&self, id: &str, _value: f64) -> bool {
            *self.last.lock().unwrap() = id.to_owned();
            true
        }
    }

    #[test]
    fn routes_prefix_rest_to_the_registered_target() {
        let router = ParameterRouter::new();
        let echo = Arc::new(Echo::default());
        router.register_target("mixer", echo.clone());
        assert!(router.set_parameter("mixer.ch1.fader", 0.5));
        assert_eq!(*echo.last.lock().unwrap(), "ch1.fader");
        assert_eq!(router.unknown_count(), 0);
    }

    #[test]
    fn ext_ids_split_the_same_way_the_router_routes_them() {
        use crate::ParamId;
        let router = ParameterRouter::new();
        let plugin = Arc::new(Echo::default());
        router.register_target("plugin", plugin.clone());
        let id = "plugin.knob.x";
        let parsed: ParamId = id.parse().expect("plugin ids parse as Ext");
        let ParamId::Ext { prefix, rest } = parsed else {
            panic!("expected Ext for {id}");
        };
        assert!(router.set_parameter(id, 0.5));
        assert_eq!(prefix, "plugin");
        assert_eq!(*plugin.last.lock().unwrap(), rest);
    }

    #[test]
    fn longest_registered_prefix_wins() {
        let router = ParameterRouter::new();
        let short = Arc::new(Echo::default());
        let long = Arc::new(Echo::default());
        router.register_target("deck", short.clone());
        router.register_target("deck.0", long.clone());
        assert!(router.set_parameter("deck.0.play", 1.0));
        assert_eq!(*long.last.lock().unwrap(), "play");
        assert!(short.last.lock().unwrap().is_empty());
        assert!(router.set_parameter("deck.1.play", 1.0));
        assert_eq!(*short.last.lock().unwrap(), "1.play");
    }

    #[test]
    fn unknown_ids_are_counted_and_not_handled() {
        let router = ParameterRouter::new();
        router.register_target("mixer", Arc::new(Echo::default()));
        assert!(!router.set_parameter("nope.x", 1.0));
        assert!(
            !router.set_parameter("mixer", 1.0),
            "bare prefix has no rest"
        );
        assert!(
            !router.set_parameter("mixerx.y", 1.0),
            "prefix must be followed by a dot"
        );
        assert_eq!(router.unknown_count(), 3);
    }

    #[test]
    fn reregistering_replaces_and_unregistering_removes() {
        let router = ParameterRouter::new();
        let first = Arc::new(Echo::default());
        let second = Arc::new(Echo::default());
        router.register_target("fx", first.clone());
        router.register_target("fx", second.clone());
        assert!(router.set_parameter("fx.wet", 0.2));
        assert_eq!(*second.last.lock().unwrap(), "wet");
        assert!(first.last.lock().unwrap().is_empty());
        router.unregister_target("fx");
        assert!(!router.set_parameter("fx.wet", 0.2));
    }

    #[test]
    fn target_may_route_a_follow_up_id_back_through_the_router() {
        struct Forwarder {
            router: Arc<ParameterRouter>,
            follow_ups: AtomicUsize,
        }
        impl ParameterTarget for Forwarder {
            fn set_parameter(&self, id: &str, _value: f64) -> bool {
                if id == "trigger" {
                    self.follow_ups.fetch_add(1, Ordering::SeqCst);
                    return self.router.set_parameter("echo.done", 1.0);
                }
                false
            }
        }
        let router = Arc::new(ParameterRouter::new());
        let echo = Arc::new(Echo::default());
        router.register_target("echo", echo.clone());
        let forwarder = Arc::new(Forwarder {
            router: router.clone(),
            follow_ups: AtomicUsize::new(0),
        });
        router.register_target("fwd", forwarder.clone());
        assert!(router.set_parameter("fwd.trigger", 1.0));
        assert_eq!(forwarder.follow_ups.load(Ordering::SeqCst), 1);
        assert_eq!(*echo.last.lock().unwrap(), "done");
    }

    #[test]
    fn set_parameter_races_with_register_and_unregister() {
        const ROUNDS: usize = 300;
        let router = ParameterRouter::new();
        let stable = Arc::new(Probe::default());
        router.register_target("stable", stable.clone());
        let stop = AtomicBool::new(false);
        let mut after_gone = 0;

        std::thread::scope(|s| {
            s.spawn(|| {
                while !stop.load(Ordering::SeqCst) {
                    router.set_parameter("stable.x", 1.0);
                    router.set_parameter("plugin.x", 0.5);
                    std::thread::sleep(Duration::from_micros(20));
                }
            });
            for _ in 0..ROUNDS {
                let target = Arc::new(Probe::default());
                router.register_target("plugin", target.clone());
                std::thread::sleep(Duration::from_micros(200));
                router.unregister_target("plugin");
                target.gone.store(true, Ordering::SeqCst);
                std::thread::sleep(Duration::from_micros(50));
                after_gone += target.after_gone.load(Ordering::SeqCst);
            }
            stop.store(true, Ordering::SeqCst);
        });

        assert_eq!(
            after_gone, 0,
            "no call may reach a target once unregister returned"
        );
        assert!(stable.calls.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn unregister_waits_for_an_in_flight_call() {
        struct Slow {
            entered: AtomicBool,
            finished: AtomicBool,
        }
        impl ParameterTarget for Slow {
            fn set_parameter(&self, _id: &str, _value: f64) -> bool {
                self.entered.store(true, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(100));
                self.finished.store(true, Ordering::SeqCst);
                true
            }
        }
        let slow = Arc::new(Slow {
            entered: AtomicBool::new(false),
            finished: AtomicBool::new(false),
        });
        let router = ParameterRouter::new();
        router.register_target("slow", slow.clone());
        std::thread::scope(|s| {
            s.spawn(|| {
                router.set_parameter("slow.x", 1.0);
            });
            while !slow.entered.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
            router.unregister_target("slow");
            assert!(
                slow.finished.load(Ordering::SeqCst),
                "unregister returned while the call was still running"
            );
        });
    }
}
