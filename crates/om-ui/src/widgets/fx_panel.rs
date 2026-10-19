//! Effects panel for one rack at a time (master or one channel): pick the unit, switch it on, set
//! the wet/dry mix and the effect's own parameters. Knobs are built from the effect's descriptors,
//! so any registered effect gets controls for free. Routes `fx.<master|ch<N>>.fx<unit>.<param>`.

use super::px;
use crate::binding::MixerBinding;
use crate::control::{Knob, Readout};
use crate::geometry::Rect;
use crate::theme::ThemeTokens;
use om_core::param_id::{FxParam, FxRack};
use om_engine::fx::{FxRackControl, FxUnitControl, ParamSpec};

/// Ranges at least this wide read as whole numbers; narrower ones read with two decimals.
pub const WIDE_RANGE: f64 = 8.0;

/// Upper-cases the first letter, as the unit and parameter labels read.
pub fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Readout for a parameter descriptor.
pub fn readout_for(spec: &ParamSpec) -> Readout {
    if f64::from(spec.max) - f64::from(spec.min) >= WIDE_RANGE {
        Readout::Whole
    } else {
        Readout::Fixed2
    }
}

/// "120.0 BPM    1 beat = 500 ms".
pub fn beat_text(bpm: f64) -> String {
    let ms = (60_000.0 / bpm.max(1.0)).round() as i64;
    format!("{bpm:.1} BPM    1 beat = {ms} ms")
}

/// Rectangles of the FX panel.
#[derive(Debug, Clone, PartialEq)]
pub struct FxLayout {
    /// One per scope: Master, then one per channel.
    pub scopes: Vec<Rect>,
    pub unit_box: Rect,
    pub enable: Rect,
    /// The mix knob first, then one per effect parameter.
    pub knobs: Vec<Rect>,
    pub beat: Rect,
}

pub struct FxPanel {
    binding: MixerBinding,
    /// 0 = master rack; `n >= 1` = channel `n - 1`.
    scope: usize,
    /// Remembered unit per scope.
    scope_units: Vec<usize>,
    pub mix: Knob,
    pub params: Vec<Knob>,
    enabled: bool,
    beat: String,
}

impl FxPanel {
    /// Starts on the master rack and reads the engine once.
    pub fn new(binding: MixerBinding) -> Self {
        let scopes = binding.num_channels() + 1;
        let mut panel = Self {
            binding,
            scope: 0,
            scope_units: vec![0; scopes],
            mix: Knob::new("Mix", Readout::Percent, 0.0, 1.0, 1.0),
            params: Vec::new(),
            enabled: false,
            beat: String::new(),
        };
        panel.rebuild_params();
        panel.refresh();
        panel
    }

    pub fn num_scopes(&self) -> usize {
        self.scope_units.len()
    }

    pub fn scope(&self) -> usize {
        self.scope
    }

    /// "Master", "Ch 1", ...
    pub fn scope_label(scope: usize) -> String {
        if scope == 0 {
            "Master".to_owned()
        } else {
            format!("Ch {scope}")
        }
    }

    /// Selects a scope (clamped), restoring its remembered unit.
    pub fn set_scope(&mut self, scope: usize) {
        self.scope = scope.min(self.num_scopes() - 1);
        self.rebuild_params();
        self.refresh();
    }

    /// Shows the rack of channel `channel` (0-based).
    pub fn show_channel(&mut self, channel: usize) {
        self.set_scope(channel + 1);
    }

    /// Selects a unit in the current scope (clamped to the rack).
    pub fn set_unit(&mut self, unit: usize) {
        let last = self.unit_count().saturating_sub(1);
        self.scope_units[self.scope] = unit.min(last);
        self.rebuild_params();
        self.refresh();
    }

    pub fn unit_index(&self) -> usize {
        self.scope_units[self.scope]
    }

    /// The rack behind the selected scope: master, or channel `scope - 1`.
    fn rack_id(&self) -> FxRack {
        if self.scope == 0 {
            FxRack::Master
        } else {
            FxRack::Channel((self.scope - 1) as u8)
        }
    }

    /// Sends a parameter of the selected unit.
    fn send_unit(&self, p: FxParam, value: f64) -> bool {
        self.binding
            .send_fx(self.rack_id(), Some(self.unit_index() as u16), p, value)
    }

    fn rack(&self) -> Option<&FxRackControl> {
        if self.scope == 0 {
            Some(self.binding.fx().master_rack())
        } else {
            self.binding.fx().channel_rack(self.scope - 1)
        }
    }

    /// The selected unit, if the rack has one.
    pub fn unit(&self) -> Option<&FxUnitControl> {
        self.rack().and_then(|r| r.unit(self.unit_index()))
    }

    pub fn unit_count(&self) -> usize {
        self.rack().map_or(0, FxRackControl::num_units)
    }

    /// Effect names of the current rack, capitalised, in unit order.
    pub fn unit_names(&self) -> Vec<String> {
        let Some(rack) = self.rack() else {
            return Vec::new();
        };
        (0..rack.num_units())
            .filter_map(|i| rack.unit(i))
            .map(|u| capitalised(u.effect_name()))
            .collect()
    }

    /// Switches the selected unit on or off. Returns the value sent, or `None` for an empty rack.
    pub fn enable_pressed(&mut self) -> Option<f64> {
        let on = self.unit()?.is_enabled();
        let value = if on { 0.0 } else { 1.0 };
        self.send_unit(FxParam::Enable, value);
        self.refresh();
        Some(value)
    }

    pub fn mix_changed(&mut self, value: f64) -> f64 {
        let v = self.mix.range.set(value);
        self.send_unit(FxParam::Mix, v);
        v
    }

    /// A parameter knob moved. Returns the value sent, or `None` for an unknown index.
    pub fn param_changed(&mut self, index: usize, value: f64) -> Option<f64> {
        let name = self.unit()?.params().spec(index)?.name;
        let stored = self.params.get_mut(index)?.range.set(value);
        self.send_unit(FxParam::Effect(name.to_owned()), stored);
        Some(stored)
    }

    /// One knob per parameter of the selected effect, at the descriptor's default until refreshed.
    fn rebuild_params(&mut self) {
        let knobs: Vec<Knob> = match self.unit() {
            Some(unit) => {
                let params = unit.params();
                (0..params.len())
                    .filter_map(|i| params.spec(i))
                    .map(|spec| {
                        Knob::new(
                            capitalised(spec.name),
                            readout_for(spec),
                            f64::from(spec.min),
                            f64::from(spec.max),
                            f64::from(spec.default),
                        )
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        self.params = knobs;
    }

    /// Reads the selected unit into the controls. Nothing is sent.
    pub fn refresh(&mut self) {
        let Some(unit) = self.unit() else {
            return;
        };
        let enabled = unit.is_enabled();
        let mix = f64::from(unit.mix());
        let targets: Vec<Option<f64>> = (0..self.params.len())
            .map(|i| unit.params().target(i).map(f64::from))
            .collect();
        let beat = beat_text(unit.tempo());
        self.enabled = enabled;
        self.mix.set_quiet(mix);
        for (knob, target) in self.params.iter_mut().zip(targets) {
            if let Some(v) = target {
                knob.set_quiet(v);
            }
        }
        self.beat = beat;
    }

    /// Whether the selected unit is switched on, as last refreshed.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The beat readout, as last refreshed.
    pub fn beat_label(&self) -> &str {
        &self.beat
    }

    /// Rectangles for `scopes` buttons, the unit row, the knobs (`params` + mix) and the beat line.
    pub fn layout(&self, bounds: Rect, theme: &ThemeTokens) -> FxLayout {
        let h = px(theme.mixer_button_height);
        let gap = theme.inner_gap;
        let mut area = bounds.reduced(theme.pad, theme.pad);

        let mut scope_row = area.remove_top(h);
        let cell = scope_row.w / self.num_scopes().max(1) as i32;
        let scopes = (0..self.num_scopes())
            .map(|_| scope_row.remove_left(cell).reduced(1, 0))
            .collect();
        area.remove_top(gap);

        let mut row = area.remove_top(h);
        let unit_box = row.remove_left(px(theme.mixer_fx_box_width));
        row.remove_left(gap);
        let enable = row.remove_left(px(theme.mixer_fx_enable_width));
        area.remove_top(gap);

        let knob_h = px(theme.mixer_knob_height);
        let mut knob_row = area.remove_top(knob_h * 3 / 2);
        let count = self.params.len() as i32 + 1;
        let knob_w = (knob_row.w / count).min(knob_h * 2);
        let knobs = (0..count).map(|_| knob_row.remove_left(knob_w)).collect();
        area.remove_top(gap);

        let beat = area.remove_top(h);
        FxLayout {
            scopes,
            unit_box,
            enable,
            knobs,
            beat,
        }
    }
}
