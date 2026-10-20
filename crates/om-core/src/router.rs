use crate::command::{Command, CommandError, CommandSink};
use crate::param_id::{is_reserved_prefix, Namespace, ParamError, ParamId};
use crate::sync::LockExt;
use crate::{ParamTarget, ParameterTarget};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, ThreadId};

/// Routes parameter ids to targets. Engine ids are parsed once into [`ParamId`] and dispatched to the
/// typed target registered for their [`Namespace`]. Plugin ids (`ParamId::Ext`) and ids the grammar
/// rejects go to the string registry, which matches raw prefixes (longest prefix wins). Engine
/// namespaces are reserved: they cannot be registered as string prefixes.
///
/// Registration and dispatch are serialised by a reentrant gate that is held across the target call,
/// so [`ParameterRouter::unregister_target`] returns only once no call into that target is in flight.
/// A target may route a follow-up id back through the router on the same thread. Never call from the
/// audio thread.
#[derive(Default)]
pub struct ParameterRouter {
    gate: Gate,
    typed: Mutex<BTreeMap<Namespace, Arc<dyn ParamTarget>>>,
    targets: Mutex<BTreeMap<String, Arc<dyn ParameterTarget>>>,
    unknown: AtomicUsize,
}

impl ParameterRouter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the typed target for an engine namespace. Re-registering replaces it.
    pub fn register_typed(&self, namespace: Namespace, target: Arc<dyn ParamTarget>) {
        let _gate = self.gate.enter();
        lock(&self.typed).insert(namespace, target);
    }

    /// Removes the typed target for `namespace`, waiting for any call already routed to it.
    pub fn unregister_typed(&self, namespace: Namespace) {
        let _gate = self.gate.enter();
        lock(&self.typed).remove(&namespace);
    }

    /// Registers `target` under a string `prefix`. Re-registering a prefix replaces its target.
    /// Engine namespaces are refused: their ids never reach string targets.
    pub fn register_target(
        &self,
        prefix: &str,
        target: Arc<dyn ParameterTarget>,
    ) -> Result<(), ParamError> {
        if is_reserved_prefix(prefix) {
            return Err(ParamError::Reserved(prefix.to_owned()));
        }
        let _gate = self.gate.enter();
        lock(&self.targets).insert(prefix.to_owned(), target);
        Ok(())
    }

    /// Removes the target under `prefix`, waiting for any call already routed to it to finish.
    pub fn unregister_target(&self, prefix: &str) {
        let _gate = self.gate.enter();
        lock(&self.targets).remove(prefix);
    }

    /// Routes an already-parsed id. Returns `false` when no target handles it (and counts it).
    pub fn dispatch(&self, id: &ParamId, value: f64) -> bool {
        let _gate = self.gate.enter();
        let handled = self.route(id, value);
        self.count_if_unhandled(handled)
    }

    /// Number of ids that were not handled by any target.
    pub fn unknown_count(&self) -> usize {
        self.unknown.load(Ordering::Relaxed)
    }

    fn route(&self, id: &ParamId, value: f64) -> bool {
        match id {
            ParamId::Ext { .. } => self.route_string(&id.to_string(), value),
            _ => {
                let target = id
                    .namespace()
                    .and_then(|ns| lock(&self.typed).get(&ns).cloned());
                target.is_some_and(|target| target.set_param(id, value))
            }
        }
    }

    fn route_string(&self, id: &str, value: f64) -> bool {
        let routed = {
            let targets = lock(&self.targets);
            // Longest registered prefix wins ("plug.0" beats "plug").
            targets
                .iter()
                .filter_map(|(prefix, target)| {
                    let rest = id.strip_prefix(prefix.as_str())?.strip_prefix('.')?;
                    Some((prefix.len(), Arc::clone(target), rest))
                })
                .max_by_key(|(len, _, _)| *len)
                .map(|(_, target, rest)| (target, rest))
        };
        match routed {
            Some((target, rest)) => target.set_parameter(rest, value),
            None => false,
        }
    }

    fn count_if_unhandled(&self, handled: bool) -> bool {
        if !handled {
            self.unknown.fetch_add(1, Ordering::Relaxed);
        }
        handled
    }
}

impl ParameterTarget for ParameterRouter {
    /// Parses `id` once. Engine ids dispatch typed; plugin ids and ids the grammar rejects fall to
    /// the string registry, which handles any raw prefix it knows.
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        let _gate = self.gate.enter();
        let handled = match id.parse::<ParamId>() {
            Ok(parsed) if parsed.namespace().is_some() => self.route(&parsed, value),
            _ => self.route_string(id, value),
        };
        self.count_if_unhandled(handled)
    }
}

impl CommandSink for ParameterRouter {
    fn send(&self, cmd: Command) -> Result<(), CommandError> {
        match cmd {
            Command::Param { id, value } => {
                if self.dispatch(&id, value) {
                    Ok(())
                } else {
                    Err(CommandError::Rejected(format!("no target for {id}")))
                }
            }
            Command::Automix(c) => {
                let id = ParamId::Automix(c.p);
                if self.dispatch(&id, c.value) {
                    Ok(())
                } else {
                    Err(CommandError::Rejected(format!("no target for {id}")))
                }
            }
            _ => Err(CommandError::Rejected(
                "command variant not wired to the engine yet".to_owned(),
            )),
        }
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
        router.register_target("plug", echo.clone()).unwrap();
        assert!(router.set_parameter("plug.ch1.fader", 0.5));
        assert_eq!(*echo.last.lock().unwrap(), "ch1.fader");
        assert_eq!(router.unknown_count(), 0);
    }

    #[test]
    fn ext_ids_split_the_same_way_the_router_routes_them() {
        use crate::ParamId;
        let router = ParameterRouter::new();
        let plugin = Arc::new(Echo::default());
        router.register_target("plugin", plugin.clone()).unwrap();
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
        router.register_target("plug", short.clone()).unwrap();
        router.register_target("plug.0", long.clone()).unwrap();
        assert!(router.set_parameter("plug.0.play", 1.0));
        assert_eq!(*long.last.lock().unwrap(), "play");
        assert!(short.last.lock().unwrap().is_empty());
        assert!(router.set_parameter("plug.1.play", 1.0));
        assert_eq!(*short.last.lock().unwrap(), "1.play");
    }

    #[test]
    fn unknown_ids_are_counted_and_not_handled() {
        let router = ParameterRouter::new();
        router
            .register_target("plug", Arc::new(Echo::default()))
            .unwrap();
        assert!(!router.set_parameter("nope.x", 1.0));
        assert!(
            !router.set_parameter("plug", 1.0),
            "bare prefix has no rest"
        );
        assert!(
            !router.set_parameter("plugx.y", 1.0),
            "prefix must be followed by a dot"
        );
        assert_eq!(router.unknown_count(), 3);
    }

    #[test]
    fn reregistering_replaces_and_unregistering_removes() {
        let router = ParameterRouter::new();
        let first = Arc::new(Echo::default());
        let second = Arc::new(Echo::default());
        router.register_target("plug", first.clone()).unwrap();
        router.register_target("plug", second.clone()).unwrap();
        assert!(router.set_parameter("plug.wet", 0.2));
        assert_eq!(*second.last.lock().unwrap(), "wet");
        assert!(first.last.lock().unwrap().is_empty());
        router.unregister_target("plug");
        assert!(!router.set_parameter("plug.wet", 0.2));
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
        router.register_target("echo", echo.clone()).unwrap();
        let forwarder = Arc::new(Forwarder {
            router: router.clone(),
            follow_ups: AtomicUsize::new(0),
        });
        router.register_target("fwd", forwarder.clone()).unwrap();
        assert!(router.set_parameter("fwd.trigger", 1.0));
        assert_eq!(forwarder.follow_ups.load(Ordering::SeqCst), 1);
        assert_eq!(*echo.last.lock().unwrap(), "done");
    }

    #[test]
    fn set_parameter_races_with_register_and_unregister() {
        const ROUNDS: usize = 300;
        let router = ParameterRouter::new();
        let stable = Arc::new(Probe::default());
        router.register_target("stable", stable.clone()).unwrap();
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
                router.register_target("plugin", target.clone()).unwrap();
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
        router.register_target("slow", slow.clone()).unwrap();
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

#[cfg(test)]
mod typed_tests {
    use super::ParameterRouter;
    use crate::command::{Command, CommandError, CommandSink};
    use crate::param_id::{DeckParam, MixerParam, Namespace};
    use crate::{ParamError, ParamId, ParamTarget, ParameterTarget};
    use std::sync::{Arc, Mutex};

    /// Records every typed id it is handed and accepts only the namespace it stands for.
    struct Recorder {
        namespace: Namespace,
        seen: Mutex<Vec<(ParamId, f64)>>,
    }

    impl Recorder {
        fn new(namespace: Namespace) -> Arc<Self> {
            Arc::new(Self {
                namespace,
                seen: Mutex::new(Vec::new()),
            })
        }

        fn seen(&self) -> Vec<(ParamId, f64)> {
            self.seen.lock().unwrap().clone()
        }
    }

    impl ParamTarget for Recorder {
        fn set_param(&self, id: &ParamId, value: f64) -> bool {
            if id.namespace() != Some(self.namespace) {
                return false;
            }
            self.seen.lock().unwrap().push((id.clone(), value));
            true
        }
    }

    #[test]
    fn string_and_typed_dispatch_reach_the_same_target_with_the_parsed_id() {
        let router = ParameterRouter::new();
        let mixer = Recorder::new(Namespace::Mixer);
        router.register_typed(Namespace::Mixer, mixer.clone());

        assert!(router.set_parameter("mixer.ch1.fader", 0.5));
        let parsed: ParamId = "mixer.ch1.fader".parse().unwrap();
        assert!(router.dispatch(&parsed, 0.5));

        let seen = mixer.seen();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].0, parsed);
        assert_eq!(seen[1].0, parsed);
        assert_eq!(router.unknown_count(), 0);
    }

    #[test]
    fn typed_namespaces_are_routed_by_variant_not_by_prefix_registry() {
        let router = ParameterRouter::new();
        let deck = Recorder::new(Namespace::Deck);
        router.register_typed(Namespace::Deck, deck.clone());
        assert!(router.set_parameter("deck.1.hotcue3.clear", 1.0));
        assert_eq!(
            deck.seen()[0].0,
            ParamId::Deck {
                deck: 1,
                p: DeckParam::Hotcue {
                    index: 3,
                    clear: true
                }
            }
        );
    }

    #[test]
    fn engine_ids_without_a_typed_target_are_unknown_and_counted() {
        let router = ParameterRouter::new();
        assert!(!router.set_parameter("deck.0.play", 1.0));
        assert!(!router.set_parameter("stem.0.0.volume", 1.0));
        assert_eq!(router.unknown_count(), 2);
    }

    #[test]
    fn malformed_engine_ids_are_unknown_and_counted() {
        let router = ParameterRouter::new();
        let mixer = Recorder::new(Namespace::Mixer);
        router.register_typed(Namespace::Mixer, mixer.clone());
        assert!(!router.set_parameter("mixer.ch9.fader", 1.0));
        assert!(!router.set_parameter("mixer.ch1.nope", 1.0));
        assert!(mixer.seen().is_empty());
        assert_eq!(router.unknown_count(), 2);
    }

    #[test]
    fn reserved_prefixes_cannot_be_registered_as_strings() {
        let router = ParameterRouter::new();
        let plug = Arc::new(Dummy);
        for prefix in [
            "deck",
            "deck.0",
            "mixer",
            "fx.master",
            "sync",
            "stem",
            "sampler",
            "automix",
        ] {
            assert_eq!(
                router.register_target(prefix, plug.clone()),
                Err(ParamError::Reserved(prefix.to_owned())),
                "{prefix}"
            );
        }
        assert!(router.register_target("plugin", plug.clone()).is_ok());
        assert!(
            router.register_target("decks", plug).is_ok(),
            "not a namespace"
        );
    }

    #[test]
    fn command_param_goes_through_the_typed_path() {
        let router = ParameterRouter::new();
        let mixer = Recorder::new(Namespace::Mixer);
        router.register_typed(Namespace::Mixer, mixer.clone());
        let id = ParamId::Mixer(MixerParam::Crossfader);
        router
            .send(Command::Param {
                id: id.clone(),
                value: 0.25,
            })
            .expect("mixer accepts crossfader");
        assert_eq!(mixer.seen(), vec![(id, 0.25)]);

        let missing = router.send(Command::Param {
            id: ParamId::Deck {
                deck: 0,
                p: DeckParam::Play,
            },
            value: 1.0,
        });
        assert!(matches!(missing, Err(CommandError::Rejected(_))));
    }

    #[test]
    fn unhandled_command_variants_are_rejected_for_now() {
        let router = ParameterRouter::new();
        let stems = crate::command::StemSetRef(1);
        let err = router.send(Command::LoadStems {
            deck: 0,
            set: stems,
        });
        assert!(matches!(err, Err(CommandError::Rejected(_))));
    }

    /// A string target that must never be reached for engine ids.
    struct Dummy;

    impl ParameterTarget for Dummy {
        fn set_parameter(&self, _id: &str, _value: f64) -> bool {
            true
        }
    }
}
