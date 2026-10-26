//! The mixer: one channel strip per deck beside the waveform stage, and the mixer module between the
//! decks' controls (page tabs, master, crossfader, headphone cue); the effects page replaces the
//! stage. Page state, refresh and the module layout. The frame clock belongs to the gpui layer.

use super::channel::ChannelStrip;
use super::crossfader::CrossfaderSection;
use super::cue::{CueSection, CUE_HEIGHT};
use super::fx_panel::FxPanel;
use super::master::MasterSection;
use crate::binding::{LevelSource, MixerBinding};
use crate::geometry::Rect;
use crate::meter::MeterParams;
use crate::theme::ThemeTokens;

/// A stalled UI should not blank the meters in one step.
pub const MAX_FRAME_SECONDS: f32 = 0.25;

/// Which page the mixer slot shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Mixer,
    Fx,
}

/// Height and width of the FX toggle at the module's top, in px.
pub const FX_TOGGLE_HEIGHT: i32 = 32;
pub const FX_TOGGLE_WIDTH: i32 = 64;

/// Rectangles of the mixer module.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleLayout {
    /// The FX toggle: lit while the FX page replaces the stage. There is no "Mixer" tab; the
    /// strips and sections are always on screen.
    pub fx: Rect,
    pub master: Rect,
    pub crossfader: Rect,
    pub cue: Rect,
}

pub struct MixerPanel {
    binding: MixerBinding,
    page: Page,
    pub strips: Vec<ChannelStrip>,
    pub master: MasterSection,
    pub crossfader: CrossfaderSection,
    pub cue: CueSection,
    pub fx: FxPanel,
}

impl MixerPanel {
    /// Builds every control for the engine's channels and shows the mixer page.
    pub fn new(binding: MixerBinding, meter: MeterParams) -> Self {
        let strips = (0..binding.num_channels())
            .map(|ch| ChannelStrip::new(binding.clone(), ch, meter))
            .collect();
        Self {
            page: Page::Mixer,
            strips,
            master: MasterSection::new(binding.clone(), meter),
            crossfader: CrossfaderSection::new(binding.clone()),
            cue: CueSection::new(binding.clone()),
            fx: FxPanel::new(binding.clone()),
            binding,
        }
    }

    pub fn page(&self) -> Page {
        self.page
    }

    /// Mixer strips and sections are shown on the mixer page only.
    pub fn mixer_visible(&self) -> bool {
        self.page == Page::Mixer
    }

    pub fn fx_visible(&self) -> bool {
        self.page == Page::Fx
    }

    pub fn set_page(&mut self, page: Page) {
        self.page = page;
        if page == Page::Fx {
            self.fx.refresh();
        }
    }

    /// A strip's FX button: show that channel's rack on the FX page.
    pub fn request_fx(&mut self, channel: usize) {
        self.fx.show_channel(channel);
        self.set_page(Page::Fx);
    }

    /// One frame: mirrors the engine and advances the meters by `dt` seconds (clamped to
    /// [`MAX_FRAME_SECONDS`]). The strips and the module stay on screen on the FX page, so they
    /// keep updating; the effects are refreshed as well while that page is open.
    pub fn refresh(&mut self, dt: f32, levels: &dyn LevelSource, dragging_fader: bool) {
        if self.page == Page::Fx {
            self.fx.refresh();
        }
        let dt = dt.clamp(0.0, MAX_FRAME_SECONDS);
        for strip in &mut self.strips {
            strip.refresh(dt, levels, dragging_fader);
        }
        self.master.refresh(dt, levels, dragging_fader);
        self.crossfader.pull(dragging_fader);
        self.cue.pull(false, false);
    }

    pub fn binding(&self) -> &MixerBinding {
        &self.binding
    }

    /// Layout of the mixer module: the crossfader block at the foot; above it the master fader and
    /// meter in a column on the left, and on the right the FX toggle at the top and the headphone
    /// knobs at the bottom (`faders-meters.md` section 3 master anatomy, `labels-layout.md`
    /// section 4 wireframe: master, headphones, crossfader, curve).
    pub fn module_layout(bounds: Rect, theme: &ThemeTokens) -> ModuleLayout {
        let gap = theme.inner_gap;
        let mut area = bounds;
        let crossfader = area.remove_bottom(CrossfaderSection::height_for(bounds.w, theme));
        area.remove_bottom(gap);
        let master = area.remove_left(super::master::width_for(theme).min(area.w));
        let mut right = area;
        let pad = theme.mixer_pad_x as i32;
        let mut inset = right.reduced(pad.min(right.w / 2), 0);
        let mut top = inset.remove_top(FX_TOGGLE_HEIGHT);
        let fx = top.remove_left(FX_TOGGLE_WIDTH.min(top.w));
        right.remove_top(FX_TOGGLE_HEIGHT + gap);
        let cue = right.remove_bottom(CUE_HEIGHT);
        ModuleLayout {
            fx,
            master,
            crossfader,
            cue,
        }
    }
}
