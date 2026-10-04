use glam::Vec3;

const ARC_SEGMENT_LENGTH_MM: f32 = 0.5;
const LAYER_Z_EPSILON: f32 = 1e-4;
const MM_PER_INCH: f32 = 25.4;
const FIT_TRIM_FRACTION: f32 = 0.005;
const SECONDS_PER_MINUTE: f32 = 60.0;
/// Features beyond this many distinct names share the last colour bucket.
pub const MAX_FEATURES: usize = 12;

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
    /// Feed rate in mm/s, 0 until the first `F` word.
    pub speed: f32,
    /// Index into `Toolpath::feature_names`, 0 when the file has no feature comments.
    pub feature: u8,
    /// Active tool (`T<n>`), 0 until the first tool command.
    pub tool: u8,
}

#[derive(Default, Debug)]
pub struct Toolpath {
    pub segments: Vec<Segment>,
    pub layer_count: u32,
    pub min: Vec3,
    pub max: Vec3,
    /// Bounds for framing the camera: X and Y exclude the outermost extrusion
    /// points so a distant purge line does not shrink the model in view.
    pub fit_min: Vec3,
    pub fit_max: Vec3,
    pub filament_mm: f32,
    /// Distinct names from `;TYPE:` or `; FEATURE:` comments, in order of first use.
    pub feature_names: Vec<String>,
    pub speed_min: f32,
    pub speed_max: f32,
    /// Print time reported by the slicer in a comment, if any.
    pub estimated_seconds: Option<u32>,
    pub extrude_move_count: usize,
    pub travel_move_count: usize,
    pub travel_distance_mm: f32,
    /// Filament colours from the slicer's `filament_colour` comment, indexed by tool.
    pub filament_colors: Vec<Option<[f32; 3]>>,
    pub filament_types: Vec<String>,
    pub filament_mm_by_tool: [f32; MAX_FEATURES],
    /// Switches between tools after printing has started.
    pub tool_change_count: usize,
    /// `M600` filament change pauses.
    pub manual_change_count: usize,
}

struct ParserState {
    position: Vec3,
    extruder: f32,
    relative_xyz: bool,
    relative_extruder: bool,
    unit_scale: f32,
    feedrate_mm_s: f32,
    feature: u8,
    tool: u8,
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
            feedrate_mm_s: 0.0,
            feature: 0,
            tool: 0,
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
            self.toolpath.filament_mm_by_tool[usize::from(self.tool)] += extruded;
        }
        self.toolpath.segments.push(Segment {
            from: self.position,
            to,
            kind,
            layer: self.layer,
            speed: self.feedrate_mm_s,
            feature: self.feature,
            tool: self.tool,
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

    fn observe_comment(&mut self, line: &str) {
        let line = line.trim();
        if let Some(name) = feature_name(line) {
            self.set_feature(name);
        } else if let Some(seconds) = estimated_seconds(line) {
            self.toolpath.estimated_seconds = Some(seconds);
        } else if let Some(list) = line.strip_prefix("; filament_colour =") {
            self.toolpath.filament_colors = parse_filament_colors(list);
        } else if let Some(list) = line.strip_prefix("; filament_type =") {
            self.toolpath.filament_types = list.split(';').map(|name| name.trim().to_string()).collect();
        }
    }

    fn select_tool(&mut self, tool: u8) {
        let tool = tool.min((MAX_FEATURES - 1) as u8);
        if tool != self.tool && self.toolpath.filament_mm > 0.0 {
            self.toolpath.tool_change_count += 1;
        }
        self.tool = tool;
    }

    fn set_feature(&mut self, name: &str) {
        let names = &mut self.toolpath.feature_names;
        let index = match names.iter().position(|known| known == name) {
            Some(index) => index,
            None => {
                names.push(name.to_string());
                names.len() - 1
            }
        };
        self.feature = index.min(MAX_FEATURES - 1) as u8;
    }

    fn apply_feedrate(&mut self, words: &Words) {
        if let Some(feedrate) = words.f {
            self.feedrate_mm_s = feedrate * self.unit_scale / SECONDS_PER_MINUTE;
        }
    }

    fn execute(&mut self, command: &str, words: &Words) {
        if matches!(command, "G0" | "G1" | "G2" | "G3") {
            self.apply_feedrate(words);
        }
        if let Some(tool) = command.strip_prefix('T').and_then(|number| number.parse::<u8>().ok()) {
            self.select_tool(tool);
            return;
        }
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
            "M600" => self.toolpath.manual_change_count += 1,
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
    f: Option<f32>,
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
            'F' => self.f = Some(value),
            _ => {}
        }
    }
}

fn parse_filament_colors(list: &str) -> Vec<Option<[f32; 3]>> {
    list.split(';').map(|entry| parse_hex_color(entry.trim())).collect()
}

fn parse_hex_color(entry: &str) -> Option<[f32; 3]> {
    let digits = entry.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&digits[range], 16).ok().map(|value| f32::from(value) / 255.0)
    };
    Some([channel(0..2)?, channel(2..4)?, channel(4..6)?])
}

fn feature_name(line: &str) -> Option<&str> {
    line.strip_prefix(";TYPE:")
        .or_else(|| line.strip_prefix("; FEATURE:"))
        .map(str::trim)
        .filter(|name| !name.is_empty())
}

fn parse_duration_seconds(text: &str) -> Option<u32> {
    let mut total = 0u32;
    let mut any = false;
    for token in text.split_whitespace() {
        let (number, unit) = token.split_at(token.len().checked_sub(1)?);
        let value: u32 = number.parse().ok()?;
        let unit_seconds = match unit {
            "d" => 86_400,
            "h" => 3_600,
            "m" => 60,
            "s" => 1,
            _ => return None,
        };
        total += value * unit_seconds;
        any = true;
    }
    any.then_some(total)
}

fn estimated_seconds(line: &str) -> Option<u32> {
    if let Some(rest) = line.strip_prefix("; estimated printing time (normal mode) =") {
        return parse_duration_seconds(rest);
    }
    line.strip_prefix(";TIME:")?.trim().parse::<f32>().ok().map(|seconds| seconds as u32)
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

fn trimmed_range(mut values: Vec<f32>) -> (f32, f32) {
    let last = values.len() - 1;
    let trim = (last as f32 * FIT_TRIM_FRACTION) as usize;
    let low = *values.select_nth_unstable_by(trim, f32::total_cmp).1;
    let high = *values.select_nth_unstable_by(last - trim, f32::total_cmp).1;
    (low, high)
}

fn compute_statistics(toolpath: &mut Toolpath) {
    let mut speed_min = f32::MAX;
    let mut speed_max = 0.0f32;
    for segment in &toolpath.segments {
        match segment.kind {
            MoveKind::Extrude => {
                toolpath.extrude_move_count += 1;
                speed_min = speed_min.min(segment.speed);
                speed_max = speed_max.max(segment.speed);
            }
            MoveKind::Travel => {
                toolpath.travel_move_count += 1;
                toolpath.travel_distance_mm += segment.from.distance(segment.to);
            }
        }
    }
    toolpath.speed_min = if speed_min == f32::MAX { 0.0 } else { speed_min };
    toolpath.speed_max = speed_max;
}

fn compute_bounds(toolpath: &mut Toolpath) {
    let points: Vec<Vec3> = toolpath
        .segments
        .iter()
        .filter(|segment| segment.kind == MoveKind::Extrude)
        .map(|segment| segment.to)
        .collect();
    let Some(first) = points.first().copied() else {
        return;
    };
    let (min, max) = points
        .iter()
        .fold((first, first), |(min, max), p| (min.min(*p), max.max(*p)));
    toolpath.min = min;
    toolpath.max = max;
    let (fit_min_x, fit_max_x) = trimmed_range(points.iter().map(|p| p.x).collect());
    let (fit_min_y, fit_max_y) = trimmed_range(points.iter().map(|p| p.y).collect());
    toolpath.fit_min = Vec3::new(fit_min_x, fit_min_y, min.z);
    toolpath.fit_max = Vec3::new(fit_max_x, fit_max_y, max.z);
}

pub fn parse_gcode(source: &str) -> Toolpath {
    let mut state = ParserState::new();
    for line in source.lines() {
        if line.starts_with(';') {
            state.observe_comment(line);
            continue;
        }
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
    compute_statistics(&mut toolpath);
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
    fn should_track_feed_rate_in_mm_per_second() {
        let toolpath = parse_gcode("G1 X5 E1 F1200\nG1 X10 E2\nG1 X15 E3 F6000\n");
        let speeds: Vec<f32> = toolpath.segments.iter().map(|segment| segment.speed).collect();
        assert_eq!(speeds, vec![20.0, 20.0, 100.0]);
        assert_eq!((toolpath.speed_min, toolpath.speed_max), (20.0, 100.0));
    }

    #[test]
    fn should_group_segments_by_feature_comment() {
        let toolpath = parse_gcode(
            ";TYPE:Outer wall\nG1 X5 E1\n; FEATURE: Infill\nG1 X10 E2\n;TYPE:Outer wall\nG1 X15 E3\n",
        );
        assert_eq!(toolpath.feature_names, vec!["Outer wall", "Infill"]);
        let features: Vec<u8> = toolpath.segments.iter().map(|segment| segment.feature).collect();
        assert_eq!(features, vec![0, 1, 0]);
    }

    #[test]
    fn should_parse_slicer_estimated_time() {
        let toolpath = parse_gcode("G1 X5 E1\n; estimated printing time (normal mode) = 1d 2h 3m 4s\n");
        assert_eq!(toolpath.estimated_seconds, Some(86_400 + 7_200 + 180 + 4));
        assert_eq!(parse_duration_seconds("garbage"), None);
    }

    #[test]
    fn should_track_tools_and_filament_per_tool() {
        let toolpath = parse_gcode(
            "T0\nG1 X5 E1\nT1\nG1 X10 E3\nT0\nG1 X15 E4\nM600\n",
        );
        let tools: Vec<u8> = toolpath.segments.iter().map(|segment| segment.tool).collect();
        assert_eq!(tools, vec![0, 1, 0]);
        assert_eq!(toolpath.tool_change_count, 2);
        assert_eq!(toolpath.manual_change_count, 1);
        assert!((toolpath.filament_mm_by_tool[1] - 2.0).abs() < 1e-5);
    }

    #[test]
    fn should_not_count_initial_tool_selection_as_a_change() {
        let toolpath = parse_gcode("T2\nG1 X5 E1\n");
        assert_eq!(toolpath.tool_change_count, 0);
    }

    #[test]
    fn should_parse_slicer_filament_colours_and_types() {
        let toolpath = parse_gcode(
            "G1 X5 E1\n; filament_colour = #FF0000;;#00FF80\n; filament_type = PLA;PETG;TPU\n",
        );
        assert_eq!(toolpath.filament_colors[0], Some([1.0, 0.0, 0.0]));
        assert_eq!(toolpath.filament_colors[1], None);
        assert!((toolpath.filament_colors[2].unwrap()[1] - 1.0).abs() < 1e-6);
        assert_eq!(toolpath.filament_types, vec!["PLA", "PETG", "TPU"]);
    }

    #[test]
    fn should_count_moves_and_travel_distance() {
        let toolpath = parse_gcode("G1 X10\nG1 X20 E1\nG1 X20 Y5\n");
        assert_eq!(toolpath.extrude_move_count, 1);
        assert_eq!(toolpath.travel_move_count, 2);
        assert!((toolpath.travel_distance_mm - 15.0).abs() < 1e-4);
    }

    #[test]
    fn should_exclude_distant_purge_line_from_fit_bounds() {
        let mut source = String::from("G1 X-50 Y-50 Z0.2\nG1 X-10 Y-50 E1\n");
        for step in 0..1000 {
            let x = 100.0 + (step % 40) as f32;
            let y = 100.0 + (step / 40) as f32;
            source.push_str(&format!("G1 X{x} Y{y} E{}\n", 2 + step));
        }
        let toolpath = parse_gcode(&source);
        assert_eq!(toolpath.min.x, -10.0);
        assert!(toolpath.fit_min.x >= 100.0 && toolpath.fit_min.y >= 100.0);
        assert!(toolpath.fit_max.x <= 140.0);
    }

    #[test]
    fn should_compute_bounds_from_extrusion_only() {
        let toolpath = parse_gcode("G1 X100 Y100\nG1 X10 Y10 E1\nG1 X20 Y30 E2\n");
        assert_eq!(toolpath.min, Vec3::new(10.0, 10.0, 0.0));
        assert_eq!(toolpath.max, Vec3::new(20.0, 30.0, 0.0));
    }
}
