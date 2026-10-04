use glam::Vec3;

const ARC_SEGMENT_LENGTH_MM: f32 = 0.5;
const LAYER_Z_EPSILON: f32 = 1e-4;
const MM_PER_INCH: f32 = 25.4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveKind {
    Extrude,
    Travel,
}

#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub from: Vec3,
    pub to: Vec3,
    pub kind: MoveKind,
    pub layer: u32,
}

#[derive(Default, Debug)]
pub struct Toolpath {
    pub segments: Vec<Segment>,
    pub layer_count: u32,
    pub min: Vec3,
    pub max: Vec3,
    pub filament_mm: f32,
}

struct ParserState {
    position: Vec3,
    extruder: f32,
    relative_xyz: bool,
    relative_extruder: bool,
    unit_scale: f32,
    layer: u32,
    layer_z: Option<f32>,
    toolpath: Toolpath,
}

impl ParserState {
    fn new() -> Self {
        Self {
            position: Vec3::ZERO,
            extruder: 0.0,
            relative_xyz: false,
            relative_extruder: false,
            unit_scale: 1.0,
            layer: 0,
            layer_z: None,
            toolpath: Toolpath::default(),
        }
    }

    fn push_segment(&mut self, to: Vec3, extruded: f32) {
        let kind = if extruded > 0.0 {
            MoveKind::Extrude
        } else {
            MoveKind::Travel
        };
        if kind == MoveKind::Extrude {
            self.advance_layer(to.z);
            self.toolpath.filament_mm += extruded;
        }
        self.toolpath.segments.push(Segment {
            from: self.position,
            to,
            kind,
            layer: self.layer,
        });
        self.position = to;
    }

    fn advance_layer(&mut self, z: f32) {
        match self.layer_z {
            None => self.layer_z = Some(z),
            Some(previous) if (z - previous).abs() > LAYER_Z_EPSILON => {
                self.layer += 1;
                self.layer_z = Some(z);
            }
            Some(_) => {}
        }
    }

    fn target_axis(&self, current: f32, word: Option<f32>) -> f32 {
        match word {
            None => current,
            Some(value) if self.relative_xyz => current + value * self.unit_scale,
            Some(value) => value * self.unit_scale,
        }
    }

    fn extruder_delta(&mut self, word: Option<f32>) -> f32 {
        let Some(value) = word else { return 0.0 };
        let value = value * self.unit_scale;
        if self.relative_extruder {
            return value;
        }
        let delta = value - self.extruder;
        self.extruder = value;
        delta
    }

    fn linear_move(&mut self, words: &Words) {
        let target = Vec3::new(
            self.target_axis(self.position.x, words.x),
            self.target_axis(self.position.y, words.y),
            self.target_axis(self.position.z, words.z),
        );
        let extruded = self.extruder_delta(words.e);
        if target != self.position {
            self.push_segment(target, extruded);
        }
    }

    fn arc_move(&mut self, words: &Words, clockwise: bool) {
        let target = Vec3::new(
            self.target_axis(self.position.x, words.x),
            self.target_axis(self.position.y, words.y),
            self.target_axis(self.position.z, words.z),
        );
        let extruded = self.extruder_delta(words.e);
        let Some(center) = self.arc_center(words, target, clockwise) else {
            if target != self.position {
                self.push_segment(target, extruded);
            }
            return;
        };
        self.push_arc_segments(center, target, extruded, clockwise);
    }

    fn arc_center(&self, words: &Words, target: Vec3, clockwise: bool) -> Option<(f32, f32)> {
        if words.i.is_some() || words.j.is_some() {
            let i = words.i.unwrap_or(0.0) * self.unit_scale;
            let j = words.j.unwrap_or(0.0) * self.unit_scale;
            return Some((self.position.x + i, self.position.y + j));
        }
        let radius = words.r? * self.unit_scale;
        let (dx, dy) = (target.x - self.position.x, target.y - self.position.y);
        let chord = (dx * dx + dy * dy).sqrt();
        if chord < f32::EPSILON || chord > 2.0 * radius.abs() {
            return None;
        }
        let height = (radius * radius - chord * chord / 4.0).max(0.0).sqrt();
        let side = if (radius > 0.0) == clockwise { -1.0 } else { 1.0 };
        let (mid_x, mid_y) = (
            self.position.x + dx / 2.0,
            self.position.y + dy / 2.0,
        );
        Some((
            mid_x + side * height * -dy / chord,
            mid_y + side * height * dx / chord,
        ))
    }

    fn push_arc_segments(
        &mut self,
        center: (f32, f32),
        target: Vec3,
        extruded: f32,
        clockwise: bool,
    ) {
        let start = self.position;
        let start_angle = (start.y - center.1).atan2(start.x - center.0);
        let end_angle = (target.y - center.1).atan2(target.x - center.0);
        let radius = ((start.x - center.0).powi(2) + (start.y - center.1).powi(2)).sqrt();
        let mut sweep = end_angle - start_angle;
        if clockwise && sweep >= 0.0 {
            sweep -= std::f32::consts::TAU;
        } else if !clockwise && sweep <= 0.0 {
            sweep += std::f32::consts::TAU;
        }
        let steps = ((sweep.abs() * radius / ARC_SEGMENT_LENGTH_MM).ceil() as u32).max(1);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let point = if step == steps {
                target
            } else {
                let angle = start_angle + sweep * t;
                Vec3::new(
                    center.0 + radius * angle.cos(),
                    center.1 + radius * angle.sin(),
                    start.z + (target.z - start.z) * t,
                )
            };
            self.push_segment(point, extruded / steps as f32);
        }
    }

    fn set_position(&mut self, words: &Words) {
        if let Some(e) = words.e {
            self.extruder = e * self.unit_scale;
        }
        self.position.x = words.x.map_or(self.position.x, |v| v * self.unit_scale);
        self.position.y = words.y.map_or(self.position.y, |v| v * self.unit_scale);
        self.position.z = words.z.map_or(self.position.z, |v| v * self.unit_scale);
    }

    fn execute(&mut self, command: &str, words: &Words) {
        match command {
            "G0" | "G1" => self.linear_move(words),
            "G2" => self.arc_move(words, true),
            "G3" => self.arc_move(words, false),
            "G20" => self.unit_scale = MM_PER_INCH,
            "G21" => self.unit_scale = 1.0,
            "G90" => {
                self.relative_xyz = false;
                self.relative_extruder = false;
            }
            "G91" => {
                self.relative_xyz = true;
                self.relative_extruder = true;
            }
            "G92" => self.set_position(words),
            "M82" => self.relative_extruder = false,
            "M83" => self.relative_extruder = true,
            _ => {}
        }
    }
}

#[derive(Default)]
struct Words {
    x: Option<f32>,
    y: Option<f32>,
    z: Option<f32>,
    e: Option<f32>,
    i: Option<f32>,
    j: Option<f32>,
    r: Option<f32>,
}

impl Words {
    fn set(&mut self, letter: char, value: f32) {
        match letter {
            'X' => self.x = Some(value),
            'Y' => self.y = Some(value),
            'Z' => self.z = Some(value),
            'E' => self.e = Some(value),
            'I' => self.i = Some(value),
            'J' => self.j = Some(value),
            'R' => self.r = Some(value),
            _ => {}
        }
    }
}

fn strip_comment(line: &str) -> &str {
    line.split(';').next().unwrap_or("").trim()
}

fn parse_line(line: &str) -> Option<(String, Words)> {
    let mut tokens = strip_comment(line).split_whitespace();
    let command = tokens.next()?.to_ascii_uppercase();
    let mut words = Words::default();
    for token in tokens {
        let mut chars = token.chars();
        let letter = chars.next()?.to_ascii_uppercase();
        if let Ok(value) = chars.as_str().parse::<f32>() {
            words.set(letter, value);
        }
    }
    Some((normalize_command(&command), words))
}

fn normalize_command(command: &str) -> String {
    let mut chars = command.chars();
    let Some(letter) = chars.next() else {
        return String::new();
    };
    match chars.as_str().parse::<u32>() {
        Ok(number) => format!("{letter}{number}"),
        Err(_) => command.to_string(),
    }
}

fn compute_bounds(toolpath: &mut Toolpath) {
    let mut points = toolpath
        .segments
        .iter()
        .filter(|segment| segment.kind == MoveKind::Extrude)
        .map(|segment| segment.to)
        .peekable();
    let Some(first) = points.peek().copied() else {
        return;
    };
    let (min, max) = points.fold((first, first), |(min, max), p| (min.min(p), max.max(p)));
    toolpath.min = min;
    toolpath.max = max;
}

pub fn parse_gcode(source: &str) -> Toolpath {
    let mut state = ParserState::new();
    for line in source.lines() {
        if let Some((command, words)) = parse_line(line) {
            state.execute(&command, &words);
        }
    }
    let mut toolpath = state.toolpath;
    toolpath.layer_count = if toolpath
        .segments
        .iter()
        .any(|segment| segment.kind == MoveKind::Extrude)
    {
        state.layer + 1
    } else {
        0
    };
    compute_bounds(&mut toolpath);
    toolpath
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_classify_extrude_and_travel_moves() {
        let toolpath = parse_gcode("G1 X10 Y0 F3000\nG1 X20 E1.0\n");
        assert_eq!(toolpath.segments.len(), 2);
        assert_eq!(toolpath.segments[0].kind, MoveKind::Travel);
        assert_eq!(toolpath.segments[1].kind, MoveKind::Extrude);
    }

    #[test]
    fn should_increment_layer_on_extruding_z_change() {
        let toolpath = parse_gcode("G1 Z0.2\nG1 X10 E1\nG1 Z0.4\nG1 X0 E2\n");
        assert_eq!(toolpath.layer_count, 2);
        assert_eq!(toolpath.segments.last().unwrap().layer, 1);
    }

    #[test]
    fn should_not_start_new_layer_on_z_hop_travel() {
        let toolpath = parse_gcode("G1 Z0.2\nG1 X10 E1\nG1 Z0.6\nG1 X0\nG1 Z0.2\nG1 X10 E2\n");
        assert_eq!(toolpath.layer_count, 1);
    }

    #[test]
    fn should_handle_relative_extrusion() {
        let toolpath = parse_gcode("M83\nG1 X5 E0.5\nG1 X10 E0.5\n");
        assert!((toolpath.filament_mm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn should_treat_retraction_as_travel() {
        let toolpath = parse_gcode("G1 X5 E1\nG1 E0.5\nG1 X10\n");
        assert_eq!(toolpath.segments.len(), 2);
        assert_eq!(toolpath.segments[1].kind, MoveKind::Travel);
    }

    #[test]
    fn should_reset_extruder_on_g92() {
        let toolpath = parse_gcode("G1 X5 E5\nG92 E0\nG1 X10 E1\n");
        assert_eq!(toolpath.segments[1].kind, MoveKind::Extrude);
        assert!((toolpath.filament_mm - 6.0).abs() < 1e-5);
    }

    #[test]
    fn should_ignore_comments_and_blank_lines() {
        let toolpath = parse_gcode("; header\n\nG1 X5 E1 ; inline\n");
        assert_eq!(toolpath.segments.len(), 1);
    }

    #[test]
    fn should_approximate_arc_with_multiple_segments() {
        let toolpath = parse_gcode("G1 X10 Y0\nG3 X0 Y10 I-10 J0 E1\n");
        assert!(toolpath.segments.len() > 10);
        let end = toolpath.segments.last().unwrap().to;
        assert!((end.x).abs() < 1e-3 && (end.y - 10.0).abs() < 1e-3);
    }

    #[test]
    fn should_compute_bounds_from_extrusion_only() {
        let toolpath = parse_gcode("G1 X100 Y100\nG1 X10 Y10 E1\nG1 X20 Y30 E2\n");
        assert_eq!(toolpath.min, Vec3::new(10.0, 10.0, 0.0));
        assert_eq!(toolpath.max, Vec3::new(20.0, 30.0, 0.0));
    }
}
