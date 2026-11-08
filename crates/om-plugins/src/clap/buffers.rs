//! Pre-allocated audio and event buffers for one stage. Everything is sized when the plugin is
//! activated, so a block only copies samples and calls the plugin.

use super::events::{EventList, HostEvent};
use super::host::Host;
use clack_host::events::event_types::ParamValueEvent;
use clack_host::events::spaces::CoreEventSpace;
use clack_host::prelude::*;

/// Event slots per block in each direction.
pub(super) const EVENT_CAPACITY: usize = 256;

pub(super) struct BlockBuffers {
    input: [Vec<f32>; 2],
    output: [Vec<f32>; 2],
    ports_in: AudioPorts,
    ports_out: AudioPorts,
    events_in: EventBuffer,
    events_out: EventBuffer,
    /// The events for this block, in our own type (the shared list the process block carries).
    pub(super) to_plugin: EventList,
    pub(super) from_plugin: EventList,
    frames: usize,
}

impl BlockBuffers {
    /// Allocates for blocks of up to `max_block` frames. Control thread.
    pub(super) fn new(max_block: usize) -> Self {
        Self {
            input: [vec![0.0; max_block], vec![0.0; max_block]],
            output: [vec![0.0; max_block], vec![0.0; max_block]],
            ports_in: AudioPorts::with_capacity(2, 1),
            ports_out: AudioPorts::with_capacity(2, 1),
            events_in: EventBuffer::with_capacity(EVENT_CAPACITY),
            events_out: EventBuffer::with_capacity(EVENT_CAPACITY),
            to_plugin: EventList::with_capacity(EVENT_CAPACITY),
            from_plugin: EventList::with_capacity(EVENT_CAPACITY),
            frames: 0,
        }
    }

    /// Copies the dry block in and clears the output and the event lists.
    pub(super) fn load(&mut self, left: &[f32], right: &[f32]) {
        self.frames = left.len().min(right.len()).min(self.input[0].len());
        let n = self.frames;
        self.input[0][..n].copy_from_slice(&left[..n]);
        self.input[1][..n].copy_from_slice(&right[..n]);
        self.output[0][..n].fill(0.0);
        self.output[1][..n].fill(0.0);
        self.events_out.clear();
        self.from_plugin.clear();
    }

    /// Copies the processed block out.
    pub(super) fn store(&self, left: &mut [f32], right: &mut [f32]) {
        let n = self.frames;
        left[..n].copy_from_slice(&self.output[0][..n]);
        right[..n].copy_from_slice(&self.output[1][..n]);
    }

    /// Moves `to_plugin` into the plugin's input event buffer, in time order.
    fn fill_events(&mut self) {
        self.events_in.clear();
        for &(frame, event) in self.to_plugin.iter() {
            let HostEvent::ParamValue { param_id, value } = event;
            if let Some(id) = ClapId::from_raw(param_id) {
                let e = ParamValueEvent::new(frame, id, Pckn::match_all(), value);
                self.events_in.push(&e);
            }
        }
        self.events_in.sort();
        self.to_plugin.clear();
    }

    /// Copies the plugin's output parameter events into `from_plugin`.
    fn read_events(&mut self) {
        for event in self.events_out.iter() {
            if let Some(CoreEventSpace::ParamValue(e)) = event.as_core_event() {
                if let Some(id) = e.param_id() {
                    let value = HostEvent::ParamValue {
                        param_id: id.get(),
                        value: e.value(),
                    };
                    self.from_plugin.push(e.header().time(), value);
                }
            }
        }
    }

    /// Calls the plugin on the loaded block.
    pub(super) fn run(
        &mut self,
        processor: &mut StartedPluginAudioProcessor<Host>,
        steady: u64,
    ) -> Result<ProcessStatus, PluginInstanceError> {
        self.fill_events();
        let n = self.frames;
        let [in_l, in_r] = &mut self.input;
        let [out_l, out_r] = &mut self.output;
        let inputs = self.ports_in.with_input_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                [in_l, in_r]
                    .into_iter()
                    .map(|b| InputChannel::variable(&mut b[..n])),
            ),
        }]);
        let mut outputs = self.ports_out.with_output_buffers([AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(
                [out_l, out_r].into_iter().map(|b| &mut b[..n]),
            ),
        }]);
        let status = processor.process(
            &inputs,
            &mut outputs,
            &InputEvents::from_buffer(&self.events_in),
            &mut OutputEvents::from_buffer(&mut self.events_out),
            Some(steady),
            None,
        );
        self.read_events();
        status
    }
}
