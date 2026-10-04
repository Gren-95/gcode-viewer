use crate::app_theme::weak_text;
use eframe::egui::{self, Color32, Sense, Shape, vec2};

const SEGMENT_HEIGHT: f32 = 30.0;
const SEGMENT_SPACING: f32 = 4.0;
const SEGMENT_PADDING: f32 = 2.0;
const GRADIENT_BAR_HEIGHT: f32 = 10.0;
const SWATCH_SIZE: f32 = 10.0;
const SECONDS_PER_MINUTE: u32 = 60;
const SECONDS_PER_HOUR: u32 = 3_600;

pub struct SegmentOption<'a, T> {
    pub value: T,
    pub label: &'a str,
    pub tooltip: &'a str,
    pub enabled: bool,
}

pub fn segmented_picker<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    current: &mut T,
    options: &[SegmentOption<T>],
) {
    let count = options.len() as f32;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SEGMENT_SPACING;
        ui.spacing_mut().button_padding.x = SEGMENT_PADDING;
        let width = (ui.available_width() - SEGMENT_SPACING * (count - 1.0)) / count;
        for option in options {
            let button = egui::Button::selectable(*current == option.value, option.label);
            let response = ui
                .add_enabled(option.enabled, |ui: &mut egui::Ui| {
                    ui.add_sized([width, SEGMENT_HEIGHT], button)
                })
                .on_hover_text(option.tooltip);
            if response.clicked() {
                *current = option.value;
            }
        }
    });
}

fn to_color32(color: [f32; 4]) -> Color32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(channel(color[0]), channel(color[1]), channel(color[2]))
}

pub fn gradient_bar(ui: &mut egui::Ui, stops: &[[f32; 4]], low_label: &str, high_label: &str) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), GRADIENT_BAR_HEIGHT),
        Sense::hover(),
    );
    let mut mesh = egui::Mesh::default();
    let last = (stops.len() - 1) as f32;
    for (index, stop) in stops.iter().enumerate() {
        let x = rect.left() + rect.width() * index as f32 / last;
        let color = to_color32(*stop);
        mesh.colored_vertex(egui::pos2(x, rect.top()), color);
        mesh.colored_vertex(egui::pos2(x, rect.bottom()), color);
        if index > 0 {
            let base = (index * 2) as u32;
            mesh.add_triangle(base - 2, base - 1, base);
            mesh.add_triangle(base - 1, base + 1, base);
        }
    }
    ui.painter().add(Shape::mesh(mesh));
    ui.horizontal(|ui| {
        ui.label(weak_text(low_label).small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(weak_text(high_label).small());
        });
    });
}

pub fn color_legend(ui: &mut egui::Ui, entries: &[(String, [f32; 4])]) {
    for (name, color) in entries {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(SWATCH_SIZE, SWATCH_SIZE), Sense::hover());
            ui.painter().rect_filled(rect, 2.0, to_color32(*color));
            ui.label(weak_text(name).small());
        });
    }
}

pub fn format_duration(total_seconds: u32) -> String {
    let hours = total_seconds / SECONDS_PER_HOUR;
    let minutes = (total_seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    if hours > 0 {
        format!("{hours} h {minutes} min")
    } else {
        format!("{minutes} min {} s", total_seconds % SECONDS_PER_MINUTE)
    }
}

pub fn format_count(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_format_durations() {
        assert_eq!(format_duration(53_433), "14 h 50 min");
        assert_eq!(format_duration(1_604), "26 min 44 s");
    }

    #[test]
    fn should_group_digits_in_threes() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(1_234), "1 234");
        assert_eq!(format_count(1_234_567), "1 234 567");
    }
}
