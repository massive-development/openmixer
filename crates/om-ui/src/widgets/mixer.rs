//! The mixer section for the main window's mixer slot: channel strips around the master section,
//! crossfader and headphone cue below, and an effects page. Port of `src/ui/MixerPanel.cpp` (page
//! state, refresh and layout). The frame clock belongs to the gpui layer.

use super::channel::ChannelStrip;
use super::crossfader::CrossfaderSection;
use super::cue::CueSection;
use super::fx_panel::FxPanel;
use super::master::MasterSection;
use super::px;
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

/// Rectangles of the mixer slot.
#[derive(Debug, Clone, PartialEq)]
pub struct MixerLayout {
    /// Mixer tab, FX tab.
    pub tabs: [Rect; 2],
    /// The FX page takes the whole body under the tabs.
    pub fx: Rect,
    pub cue: Rect,
    pub crossfader: Rect,
    pub strips: Vec<Rect>,
    pub master: Rect,
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
    /// [`MAX_FRAME_SECONDS`]). On the FX page only the effects are refreshed.
    pub fn refresh(&mut self, dt: f32, levels: &dyn LevelSource, dragging_fader: bool) {
        if self.page == Page::Fx {
            self.fx.refresh();
            return;
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

    /// Layout of the slot. The channels split around the master: the first half left of it, the
    /// rest right. Mirrors `MixerPanel::resized` and `layoutStrips`.
    pub fn layout(bounds: Rect, channels: usize, theme: &ThemeTokens) -> MixerLayout {
        let gap = theme.inner_gap;
        let mut area = bounds;
        let mut tab_row = area.remove_top(px(theme.mixer_tab_height));
        let tabs = [0, 1].map(|_| tab_row.remove_left(px(theme.mixer_page_button_width)));
        area.remove_top(gap);
        let fx = area;
        let cue = area.remove_bottom(px(theme.mixer_cue_height));
        area.remove_bottom(gap);
        let crossfader = area.remove_bottom(px(theme.mixer_crossfader_height));
        area.remove_bottom(gap);

        let (strips, master) = Self::layout_strips(area, channels, theme);
        MixerLayout {
            tabs,
            fx,
            cue,
            crossfader,
            strips,
            master,
        }
    }

    fn layout_strips(row: Rect, n: usize, theme: &ThemeTokens) -> (Vec<Rect>, Rect) {
        let gap = theme.inner_gap;
        let mut row = row;
        let n_i = n as i32;
        let left_count = n.div_ceil(2);
        let strip_w = if n > 0 {
            (row.w - px(theme.mixer_master_width) - n_i * gap) / n_i
        } else {
            0
        };
        let mut strips = Vec::with_capacity(n);
        let mut master = Rect::default();
        for i in 0..n {
            if i == left_count {
                master = row.remove_left(px(theme.mixer_master_width));
                row.remove_left(gap);
            }
            if i == n - 1 && left_count < n {
                strips.push(row);
            } else {
                strips.push(row.remove_left(strip_w));
            }
            row.remove_left(gap);
        }
        if left_count == n {
            master = row;
        }
        (strips, master)
    }
}
