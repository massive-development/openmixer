//! The event list the process block carries in and out. Parameter events today; note events slot
//! into the same list when the `note-ports` feature is enabled (ADR 0007).

/// One event for the plugin, at a frame offset inside the block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostEvent {
    ParamValue { param_id: u32, value: f64 },
}

/// A fixed-capacity event list. Pushing past the capacity drops the event and counts it, so the
/// audio thread never grows it.
#[derive(Debug)]
pub struct EventList {
    events: Vec<(u32, HostEvent)>,
    dropped: u64,
}

impl EventList {
    /// Allocates room for `capacity` events. Control thread.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
            dropped: 0,
        }
    }

    /// Adds an event at `frame`. False (and counted) when the list is full.
    pub fn push(&mut self, frame: u32, event: HostEvent) -> bool {
        if self.events.len() == self.events.capacity() {
            self.dropped += 1;
            return false;
        }
        self.events.push((frame, event));
        true
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &(u32, HostEvent)> {
        self.events.iter()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Events refused because the list was full, since creation.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_list_drops_and_counts_without_growing() {
        let mut list = EventList::with_capacity(2);
        let event = HostEvent::ParamValue {
            param_id: 1,
            value: 0.5,
        };
        assert!(list.push(0, event) && list.push(3, event));
        assert!(!list.push(4, event));
        assert_eq!((list.len(), list.dropped()), (2, 1));
        list.clear();
        assert!(list.is_empty());
        assert!(list.push(0, event));
    }
}
