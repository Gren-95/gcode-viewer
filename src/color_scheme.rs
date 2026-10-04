use crate::parse_gcode::MAX_FEATURES;

pub const PALETTE_STOPS: usize = 5;
const MIN_TOOL_CHANNEL: f32 = 0.07;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Height,
    Speed,
    Layer,
    Feature,
    Filament,
}

impl ColorMode {
    pub const ALL: [ColorMode; 5] = [
        Self::Height,
        Self::Speed,
        Self::Layer,
        Self::Feature,
        Self::Filament,
    ];

    /// The next mode in order, skipping feature colouring when `has_features` is false.
    pub fn next(self, available: impl Fn(ColorMode) -> bool) -> ColorMode {
        let position = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0);
        (1..=Self::ALL.len())
            .map(|step| Self::ALL[(position + step) % Self::ALL.len()])
            .find(|mode| available(*mode))
            .unwrap_or(self)
    }

    pub fn shader_index(self) -> f32 {
        match self {
            Self::Height => 0.0,
            Self::Speed => 1.0,
            Self::Layer => 2.0,
            Self::Feature => 3.0,
            Self::Filament => 4.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Height => "Height",
            Self::Speed => "Speed",
            Self::Layer => "Layer",
            Self::Feature => "Type",
            Self::Filament => "Filament",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Height => "Colour by Z height.",
            Self::Speed => "Colour by feed rate, slow to fast.",
            Self::Layer => "Colour by layer number.",
            Self::Feature => "Colour by feature type (needs ;TYPE: or ; FEATURE: comments).",
            Self::Filament => "Colour by tool, using the slicer's filament colours.",
        }
    }

    /// Gradient stops as sRGB colours, evenly spaced from 0 to 1.
    pub fn palette(self) -> [[f32; 4]; PALETTE_STOPS] {
        match self {
            Self::Height | Self::Feature | Self::Filament => HEIGHT_PALETTE,
            Self::Speed => SPEED_PALETTE,
            Self::Layer => LAYER_PALETTE,
        }
    }
}

const HEIGHT_PALETTE: [[f32; 4]; PALETTE_STOPS] = [
    [0.27, 0.52, 0.80, 1.0],
    [0.46, 0.55, 0.72, 1.0],
    [0.65, 0.56, 0.58, 1.0],
    [0.82, 0.57, 0.40, 1.0],
    [0.96, 0.58, 0.28, 1.0],
];

const SPEED_PALETTE: [[f32; 4]; PALETTE_STOPS] = [
    [0.27, 0.40, 0.85, 1.0],
    [0.20, 0.72, 0.78, 1.0],
    [0.45, 0.80, 0.40, 1.0],
    [0.95, 0.82, 0.30, 1.0],
    [0.92, 0.30, 0.25, 1.0],
];

const LAYER_PALETTE: [[f32; 4]; PALETTE_STOPS] = [
    [0.27, 0.00, 0.33, 1.0],
    [0.23, 0.32, 0.55, 1.0],
    [0.13, 0.57, 0.55, 1.0],
    [0.37, 0.79, 0.38, 1.0],
    [0.99, 0.91, 0.14, 1.0],
];

pub const FEATURE_COLORS: [[f32; 4]; MAX_FEATURES] = [
    [0.91, 0.36, 0.33, 1.0],
    [0.95, 0.60, 0.25, 1.0],
    [0.95, 0.82, 0.30, 1.0],
    [0.55, 0.78, 0.35, 1.0],
    [0.25, 0.72, 0.55, 1.0],
    [0.25, 0.70, 0.85, 1.0],
    [0.35, 0.50, 0.90, 1.0],
    [0.60, 0.45, 0.88, 1.0],
    [0.82, 0.45, 0.80, 1.0],
    [0.90, 0.50, 0.60, 1.0],
    [0.65, 0.65, 0.68, 1.0],
    [0.45, 0.45, 0.50, 1.0],
];

/// Colours per tool: the slicer's filament colour when known, otherwise a distinct palette entry.
/// Very dark colours are lifted slightly so black filament stays visible on the dark scene.
pub fn tool_palette(filament_colors: &[Option<[f32; 3]>]) -> [[f32; 4]; MAX_FEATURES] {
    let mut palette = FEATURE_COLORS;
    for (slot, color) in palette.iter_mut().zip(filament_colors) {
        if let Some([red, green, blue]) = color {
            *slot = [
                red.max(MIN_TOOL_CHANNEL),
                green.max(MIN_TOOL_CHANNEL),
                blue.max(MIN_TOOL_CHANNEL),
                1.0,
            ];
        }
    }
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_cycle_through_all_modes() {
        let all = |_: ColorMode| true;
        assert_eq!(ColorMode::Height.next(all), ColorMode::Speed);
        assert_eq!(ColorMode::Feature.next(all), ColorMode::Filament);
        assert_eq!(ColorMode::Filament.next(all), ColorMode::Height);
    }

    #[test]
    fn should_skip_unavailable_modes() {
        let without_features = |mode: ColorMode| mode != ColorMode::Feature;
        assert_eq!(ColorMode::Layer.next(without_features), ColorMode::Filament);
        let only_height = |mode: ColorMode| mode == ColorMode::Height;
        assert_eq!(ColorMode::Layer.next(only_height), ColorMode::Height);
    }

    #[test]
    fn should_use_slicer_colours_and_lift_black() {
        let palette = tool_palette(&[Some([0.0, 0.0, 0.0]), None, Some([1.0, 0.5, 0.3])]);
        assert!(palette[0][0] >= MIN_TOOL_CHANNEL && palette[0][0] < 0.2);
        assert_eq!(palette[1], FEATURE_COLORS[1]);
        assert_eq!(palette[2], [1.0, 0.5, 0.3, 1.0]);
    }
}
