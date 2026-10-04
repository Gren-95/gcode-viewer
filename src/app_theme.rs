use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke};

pub const VIEWPORT_FILL: Color32 = Color32::from_rgb(19, 21, 25);
pub const ACCENT: Color32 = Color32::from_rgb(232, 134, 60);
pub const PANEL_WIDTH: f32 = 280.0;
pub const SPACING_UNIT: f32 = 8.0;

const PANEL_FILL: Color32 = Color32::from_rgb(25, 27, 32);
const CARD_FILL: Color32 = Color32::from_rgb(33, 36, 43);
const WIDGET_FILL: Color32 = Color32::from_rgb(47, 51, 61);
const WIDGET_HOVER_FILL: Color32 = Color32::from_rgb(62, 67, 79);
const WIDGET_ACTIVE_FILL: Color32 = Color32::from_rgb(74, 80, 94);
const TEXT_STRONG: Color32 = Color32::from_rgb(236, 238, 243);
const TEXT_NORMAL: Color32 = Color32::from_rgb(200, 204, 213);
const TEXT_WEAK: Color32 = Color32::from_rgb(136, 142, 156);
const WIDGET_RADIUS: u8 = 6;
const CARD_RADIUS: u8 = 10;
const CARD_PADDING: i8 = 12;
const PANEL_PADDING: i8 = 16;

pub fn apply_app_theme(ctx: &egui::Context) {
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(SPACING_UNIT, SPACING_UNIT);
        style.spacing.button_padding = egui::vec2(SPACING_UNIT * 1.5, SPACING_UNIT * 0.75);
        style.spacing.interact_size.y = 28.0;
        style.spacing.slider_rail_height = 6.0;
        let visuals = &mut style.visuals;
        visuals.override_text_color = Some(TEXT_NORMAL);
        visuals.panel_fill = PANEL_FILL;
        visuals.selection.bg_fill = ACCENT;
        visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(25, 27, 32));
        visuals.slider_trailing_fill = true;
        visuals.widgets.noninteractive.fg_stroke.color = TEXT_NORMAL;
        let widget_radius = CornerRadius::same(WIDGET_RADIUS);
        for (widget, fill) in [
            (&mut visuals.widgets.inactive, WIDGET_FILL),
            (&mut visuals.widgets.hovered, WIDGET_HOVER_FILL),
            (&mut visuals.widgets.active, WIDGET_ACTIVE_FILL),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = Stroke::NONE;
            widget.corner_radius = widget_radius;
        }
    });
}

pub fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(PANEL_FILL)
        .inner_margin(Margin::same(PANEL_PADDING))
}

pub fn viewport_frame() -> egui::Frame {
    egui::Frame::NONE.fill(VIEWPORT_FILL)
}

pub fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(title.to_uppercase()).small().strong().color(TEXT_WEAK));
    egui::Frame::NONE
        .fill(CARD_FILL)
        .corner_radius(CornerRadius::same(CARD_RADIUS))
        .inner_margin(Margin::same(CARD_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
    ui.add_space(SPACING_UNIT);
}

pub fn heading_text(text: &str) -> RichText {
    RichText::new(text).strong().size(15.0).color(TEXT_STRONG)
}

pub fn weak_text(text: &str) -> RichText {
    RichText::new(text).color(TEXT_WEAK)
}

pub fn value_row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.label(weak_text(label));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value.into()).color(TEXT_STRONG));
        });
    });
}

pub fn slider_row<Num: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value_text: String,
    value: &mut Num,
    range: std::ops::RangeInclusive<Num>,
) -> egui::Response {
    value_row(ui, label, value_text);
    ui.spacing_mut().slider_width = ui.available_width();
    ui.add(egui::Slider::new(value, range).show_value(false))
}
