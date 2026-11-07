//! The host-side value snapshot of a plugin's parameters and the queue that carries changes to the
//! audio thread (ADR 0007, step 5).
//!
//! UI or MIDI -> `ParamSet::set` (control side, may lock) -> one `rtrb` ring per live stage ->
//! parameter events in the next `process`. Values the plugin changes itself (its GUI) come back in
//! the process output events and land in the snapshot atomics, which the host reads without a lock.

use super::params::ClapParam;
use crate::api::ParameterInfo;
use om_core::ParameterTarget;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// How many changes a stage queues before the oldest request is refused.
pub(super) const QUEUE_CAPACITY: usize = 256;

/// A change on its way to the audio thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ParamChange {
    pub param_id: u32,
    pub value: f64,
}

struct Link {
    token: u64,
    tx: rtrb::Producer<ParamChange>,
}

/// Declared parameters, their latest values, and the rings to the live stages.
pub(super) struct ParamSet {
    pub params: Vec<ClapParam>,
    values: Vec<AtomicU64>,
    links: Mutex<Vec<Link>>,
    next_token: AtomicU64,
    refused: AtomicU64,
}

impl ParamSet {
    pub(super) fn new(declared: Vec<(ClapParam, f64)>) -> Self {
        let (params, values): (Vec<_>, Vec<_>) = declared
            .into_iter()
            .map(|(p, v)| (p, AtomicU64::new(v.to_bits())))
            .unzip();
        Self {
            params,
            values,
            links: Mutex::new(Vec::new()),
            next_token: AtomicU64::new(1),
            refused: AtomicU64::new(0),
        }
    }

    fn links(&self) -> MutexGuard<'_, Vec<Link>> {
        self.links.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn index_of_id(&self, param_id: u32) -> Option<usize> {
        self.params.iter().position(|p| p.id == param_id)
    }

    pub(super) fn value(&self, index: usize) -> f64 {
        f64::from_bits(self.values[index].load(Ordering::Relaxed))
    }

    /// Records a value the plugin reported. Lock-free, safe from the audio thread.
    pub(super) fn store_from_plugin(&self, index: usize, value: f64) {
        self.values[index].store(value.to_bits(), Ordering::Relaxed);
    }

    /// Changes requests refused because a stage's ring was full, since creation.
    pub(super) fn refused(&self) -> u64 {
        self.refused.load(Ordering::Relaxed)
    }

    /// Adds a stage: returns its token and the consumer end of its ring. Control thread.
    pub(super) fn link(&self) -> (u64, rtrb::Consumer<ParamChange>) {
        let (tx, rx) = rtrb::RingBuffer::new(QUEUE_CAPACITY);
        let token = self.next_token.fetch_add(1, Ordering::Relaxed);
        self.links().push(Link { token, tx });
        (token, rx)
    }

    /// Removes a stage's ring. Control thread.
    pub(super) fn unlink(&self, token: u64) {
        self.links().retain(|l| l.token != token);
    }

    /// Clamps and records `value`, then queues it for every live stage. Control side only.
    pub(super) fn set(&self, index: usize, value: f64) {
        let p = &self.params[index];
        let value = value.clamp(p.min, p.max);
        self.values[index].store(value.to_bits(), Ordering::Relaxed);
        let change = ParamChange {
            param_id: p.id,
            value,
        };
        for link in self.links().iter_mut() {
            if link.tx.push(change).is_err() {
                self.refused.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn infos(&self) -> Vec<ParameterInfo> {
        self.params
            .iter()
            .filter(|p| !p.hidden)
            .map(|p| ParameterInfo {
                id: p.id.to_string(),
                name: p.name.clone(),
                min: p.min,
                max: p.max,
                default: p.default,
            })
            .collect()
    }
}

/// The plugin's parameter target: ids are the CLAP parameter ids in decimal.
pub(super) struct ClapTarget {
    pub set: Arc<ParamSet>,
}

impl ParameterTarget for ClapTarget {
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        let Ok(param_id) = id.parse::<u32>() else {
            return false;
        };
        match self.set.index_of_id(param_id) {
            Some(i) if value.is_finite() && !self.set.params[i].read_only => {
                self.set.set(i, value);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> Arc<ParamSet> {
        let p = ClapParam {
            id: 7,
            name: "Gain".into(),
            module: String::new(),
            min: 0.0,
            max: 2.0,
            default: 1.0,
            stepped: false,
            hidden: false,
            read_only: false,
        };
        Arc::new(ParamSet::new(vec![(p, 1.0)]))
    }

    #[test]
    fn a_change_is_clamped_recorded_and_queued_for_every_stage() {
        let set = set();
        let (a, mut rx_a) = set.link();
        let (_b, mut rx_b) = set.link();
        let target = ClapTarget {
            set: Arc::clone(&set),
        };
        assert!(target.set_parameter("7", 5.0));
        assert_eq!(set.value(0), 2.0);
        for rx in [&mut rx_a, &mut rx_b] {
            assert_eq!(
                rx.pop().ok(),
                Some(ParamChange {
                    param_id: 7,
                    value: 2.0
                })
            );
        }
        set.unlink(a);
        assert!(target.set_parameter("7", 0.5));
        assert!(rx_a.pop().is_err() && rx_b.pop().is_ok());
    }

    #[test]
    fn unknown_ids_and_non_finite_values_are_refused() {
        let target = ClapTarget { set: set() };
        assert!(!target.set_parameter("8", 1.0));
        assert!(!target.set_parameter("gain", 1.0));
        assert!(!target.set_parameter("7", f64::NAN));
    }

    #[test]
    fn a_full_ring_refuses_and_counts() {
        let set = set();
        let (_t, _rx) = set.link();
        for _ in 0..QUEUE_CAPACITY + 3 {
            set.set(0, 1.0);
        }
        assert_eq!(set.refused(), 3);
    }
}
