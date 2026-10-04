use crate::parse_gcode::MAX_FEATURES;

pub const PALETTE_STOPS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Height,
    Speed,
    Layer,
    Feature,
}

impl ColorMode {
    pub const ALL: [ColorMode; 4] = [Self::Height, Self::Speed, Self::Layer, Self::Feature];

    /// The next mode in order, skipping feature colouring when `has_features` is false.
    pub fn next(self, has_features: bool) -> ColorMode {
        let position = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0);
        let next = Self::ALL[(position + 1) % Self::ALL.len()];
        if next == Self::Feature && !has_features {
            Self::Height
        } else {
            next
        }
    }

    pub fn shader_index(self) -> f32 {
        match self {
            Self::Height => 0.0,
            Self::Speed => 1.0,
            Self::Layer => 2.0,
            Self::Feature => 3.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Height => "Height",
            Self::Speed => "Speed",
            Self::Layer => "Layer",
            Self::Feature => "Type",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Height => "Colour by Z height.",
            Self::Speed => "Colour by feed rate, slow to fast.",
            Self::Layer => "Colour by layer number.",
            Self::Feature => "Colour by feature type (needs ;TYPE: or ; FEATURE: comments).",
        }
    }

    /// Gradient stops as sRGB colours, evenly spaced from 0 to 1.
    pub fn palette(self) -> [[f32; 4]; PALETTE_STOPS] {
        match self {
            Self::Height | Self::Feature => HEIGHT_PALETTE,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_cycle_through_all_modes() {
        assert_eq!(ColorMode::Height.next(true), ColorMode::Speed);
        assert_eq!(ColorMode::Layer.next(true), ColorMode::Feature);
        assert_eq!(ColorMode::Feature.next(true), ColorMode::Height);
    }

    #[test]
    fn should_skip_feature_mode_without_feature_comments() {
        assert_eq!(ColorMode::Layer.next(false), ColorMode::Height);
    }
}
