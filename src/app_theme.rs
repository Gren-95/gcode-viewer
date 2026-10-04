use eframe::egui::collapsing_header::{CollapsingState, paint_default_icon};
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke};

pub const VIEWPORT_FILL: Color32 = Color32::from_rgb(19, 21, 25);
pub const ACCENT: Color32 = Color32::from_rgb(232, 134, 60);
pub const PANEL_WIDTH: f32 = 280.0;
pub const SPACING_UNIT: f32 = 8.0;

const PANEL_FILL: Color32 = Color32::from_rgb(25, 27, 32);
const CARD_FILL: Color32 = Color32::from_rgb(33, 36, 43);
const HEADER_FILL: Color32 = Color32::from_rgb(41, 45, 54);
const HEADER_HOVER_FILL: Color32 = Color32::from_rgb(50, 55, 66);
const HEADER_HEIGHT: f32 = 32.0;
const SUBHEADER_HEIGHT: f32 = 26.0;
const SUBCARD_FILL: Color32 = Color32::from_rgb(40, 44, 53);
const SUBCARD_RADIUS: u8 = 8;
const SUBCARD_PADDING: i8 = 10;
const HEADER_PADDING: f32 = 10.0;
const ICON_SIZE: f32 = 12.0;
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

struct CardStyle {
    card_fill: Color32,
    header_fill: Color32,
    header_hover_fill: Color32,
    radius: u8,
    header_height: f32,
    body_padding: i8,
}

const SECTION_STYLE: CardStyle = CardStyle {
    card_fill: CARD_FILL,
    header_fill: HEADER_FILL,
    header_hover_fill: HEADER_HOVER_FILL,
    radius: CARD_RADIUS,
    header_height: HEADER_HEIGHT,
    body_padding: CARD_PADDING,
};

const SUBSECTION_STYLE: CardStyle = CardStyle {
    card_fill: SUBCARD_FILL,
    header_fill: WIDGET_FILL,
    header_hover_fill: WIDGET_HOVER_FILL,
    radius: SUBCARD_RADIUS,
    header_height: SUBHEADER_HEIGHT,
    body_padding: SUBCARD_PADDING,
};

pub fn section(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    collapsible_card(ui, title, default_open, &SECTION_STYLE, add_contents);
    ui.add_space(SPACING_UNIT);
}

/// A smaller collapsible card meant to sit inside a section.
pub fn subsection(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    collapsible_card(ui, title, default_open, &SUBSECTION_STYLE, add_contents);
}

fn collapsible_card(
    ui: &mut egui::Ui,
    title: &str,
    default_open: bool,
    style: &CardStyle,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let id = ui.make_persistent_id(("collapsible_card", title));
    let mut state = CollapsingState::load_with_default_open(ui.ctx(), id, default_open);
    egui::Frame::NONE
        .fill(style.card_fill)
        .corner_radius(CornerRadius::same(style.radius))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            if card_header(ui, title, state.openness(ui.ctx()), style).clicked() {
                state.toggle(ui);
            }
            ui.spacing_mut().item_spacing.y = SPACING_UNIT;
            state.show_body_unindented(ui, |ui| {
                egui::Frame::NONE
                    .inner_margin(Margin::same(style.body_padding))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        add_contents(ui);
                    });
            });
        });
    state.store(ui.ctx());
}

fn card_header(
    ui: &mut egui::Ui,
    title: &str,
    openness: f32,
    style: &CardStyle,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), style.header_height),
        egui::Sense::click(),
    );
    let fill = if response.hovered() {
        style.header_hover_fill
    } else {
        style.header_fill
    };
    let bottom_radius = if openness > 0.0 { 0 } else { style.radius };
    let rounding = CornerRadius {
        nw: style.radius,
        ne: style.radius,
        sw: bottom_radius,
        se: bottom_radius,
    };
    ui.painter().rect_filled(rect, rounding, fill);
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + HEADER_PADDING + ICON_SIZE * 0.5, rect.center().y),
        egui::vec2(ICON_SIZE, ICON_SIZE),
    );
    let icon_response = ui.interact(icon_rect, response.id.with("icon"), egui::Sense::hover());
    paint_default_icon(ui, openness, &icon_response);
    ui.painter().text(
        egui::pos2(icon_rect.right() + HEADER_PADDING, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title.to_uppercase(),
        egui::FontId::proportional(11.0),
        TEXT_STRONG,
    );
    response
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
