//! Density: the Roomy and Compact metric sets a theme defines and the preferences choose between.
//! Compact is Roomy shifted one Fibonacci step down (`openmixer-workbench/docs/design-system/DESIGN.md`, Layout).

/// Which metric set the window lays out with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    #[default]
    Roomy,
    Compact,
}

/// The density the user asked for on the Appearance page. `Automatic` picks Roomy when the window
/// is tall enough for it and Compact otherwise; `Roomy` gives way to Compact only where it would
/// leave the library without its rows (`main_layout::resolve_density`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DensityChoice {
    #[default]
    Automatic,
    Roomy,
    Compact,
}

impl DensityChoice {
    pub const ALL: [DensityChoice; 3] = [
        DensityChoice::Automatic,
        DensityChoice::Roomy,
        DensityChoice::Compact,
    ];

    /// The preferences file's spelling.
    pub const fn id(self) -> &'static str {
        match self {
            DensityChoice::Automatic => "automatic",
            DensityChoice::Roomy => "roomy",
            DensityChoice::Compact => "compact",
        }
    }

    /// The Appearance page's label.
    pub const fn label(self) -> &'static str {
        match self {
            DensityChoice::Automatic => "Automatic",
            DensityChoice::Roomy => "Roomy",
            DensityChoice::Compact => "Compact",
        }
    }

    /// The choice spelled `id`; `None` for anything else.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// Sizes (px) that change with density. Layout reads them through [`super::ThemeTokens::metrics`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DensityMetrics {
    /// Spacing steps s1..s6, smallest first.
    pub space: [f32; 6],
    pub control_height: f32,
    pub knob_size: f32,
    pub pad_size: f32,
    pub row_height: f32,
    pub mixer_width: f32,
    /// Corner of large panels, and of controls.
    pub panel_radius: f32,
    pub control_radius: f32,
}

impl DensityMetrics {
    pub const fn roomy() -> Self {
        Self {
            space: [3.0, 5.0, 8.0, 13.0, 21.0, 34.0],
            control_height: 26.0,
            knob_size: 34.0,
            pad_size: 55.0,
            row_height: 34.0,
            mixer_width: 320.0,
            panel_radius: 3.0,
            control_radius: 2.0,
        }
    }

    pub const fn compact() -> Self {
        Self {
            space: [2.0, 3.0, 5.0, 8.0, 13.0, 21.0],
            control_height: 21.0,
            knob_size: 26.0,
            pad_size: 34.0,
            row_height: 26.0,
            mixer_width: 260.0,
            panel_radius: 2.0,
            control_radius: 2.0,
        }
    }

    /// Stores `value` under its theme-file key; `false` for a key this set does not have.
    pub(super) fn set(&mut self, key: &str, value: f32) -> bool {
        let slot = match key {
            "s1" | "s2" | "s3" | "s4" | "s5" | "s6" => {
                let i = usize::from(key.as_bytes()[1] - b'1');
                &mut self.space[i]
            }
            "controlHeight" => &mut self.control_height,
            "knobSize" => &mut self.knob_size,
            "padSize" => &mut self.pad_size,
            "rowHeight" => &mut self.row_height,
            "mixerWidth" => &mut self.mixer_width,
            "panelRadius" => &mut self.panel_radius,
            "controlRadius" => &mut self.control_radius,
            _ => return false,
        };
        *slot = value;
        true
    }

    /// Whether the spacing steps rise strictly, as a scale must.
    pub fn steps_rise(&self) -> bool {
        self.space.windows(2).all(|w| w[0] < w[1])
    }
}

/// Theme-file keys of one density set.
pub(super) const KEYS: &[&str] = &[
    "s1",
    "s2",
    "s3",
    "s4",
    "s5",
    "s6",
    "controlHeight",
    "knobSize",
    "padSize",
    "rowHeight",
    "mixerWidth",
    "panelRadius",
    "controlRadius",
];

/// Accepted range of a density key (px).
pub(super) fn range(key: &str) -> (f32, f32) {
    match key {
        "panelRadius" | "controlRadius" => (0.0, 16.0),
        "mixerWidth" => (0.0, 1000.0),
        _ => (0.0, 200.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_is_roomy_one_step_down() {
        let (roomy, compact) = (DensityMetrics::roomy(), DensityMetrics::compact());
        assert_eq!(compact.space[1..], roomy.space[..5]);
        assert!(roomy.steps_rise() && compact.steps_rise());
        assert_eq!(Density::default(), Density::Roomy);
    }

    #[test]
    fn a_density_choice_round_trips_through_its_id() {
        for choice in DensityChoice::ALL {
            assert_eq!(DensityChoice::from_id(choice.id()), Some(choice));
        }
        assert_eq!(DensityChoice::from_id("tiny"), None);
        assert_eq!(DensityChoice::default(), DensityChoice::Automatic);
    }

    #[test]
    fn every_key_sets_a_field() {
        let mut m = DensityMetrics::roomy();
        for (i, key) in KEYS.iter().enumerate() {
            assert!(m.set(key, 100.0 + i as f32), "{key}");
        }
        assert_eq!(m.space[5], 105.0);
        assert_eq!(m.control_radius, 112.0);
        assert!(!m.set("s7", 1.0));
    }
}
