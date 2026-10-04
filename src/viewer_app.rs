use crate::app_theme::{
    ACCENT, PANEL_WIDTH, apply_app_theme, heading_text, panel_frame, section, slider_row,
    value_row, viewport_frame, weak_text,
};
use crate::orbit_camera::OrbitCamera;
use crate::parse_gcode::{Toolpath, parse_gcode};
use crate::toolpath_mesh::{ToolpathMesh, build_toolpath_mesh};
use crate::toolpath_renderer::{
    LightAngles, MAX_LIGHT_ELEVATION_DEGREES, MIN_LIGHT_ELEVATION_DEGREES, RenderQuality,
    ToolpathDraw, ToolpathGpu,
};
use eframe::egui::{self, Color32, RichText};
use eframe::egui_wgpu;
use std::path::{Path, PathBuf};

const OPEN_BUTTON_HEIGHT: f32 = 36.0;
const RESET_BUTTON_HEIGHT: f32 = 32.0;
const QUALITY_BUTTON_HEIGHT: f32 = 30.0;

struct LoadedModel {
    file_name: String,
    toolpath: Toolpath,
    mesh: ToolpathMesh,
}

pub struct ViewerApp {
    camera: OrbitCamera,
    model: Option<LoadedModel>,
    first_layer: u32,
    last_layer: u32,
    show_travel: bool,
    quality: RenderQuality,
    light: LightAngles,
    error_message: Option<String>,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        apply_app_theme(&cc.egui_ctx);
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("wgpu backend is required");
        let gpu = ToolpathGpu::new(&render_state.device, render_state.target_format);
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(gpu);
        let mut app = Self {
            camera: OrbitCamera::default(),
            model: None,
            first_layer: 0,
            last_layer: 0,
            show_travel: false,
            quality: RenderQuality::Shadowed,
            light: LightAngles::default(),
            error_message: None,
        };
        if let Some(path) = initial_file {
            app.load_file(render_state, &path);
        }
        app
    }

    fn load_file(&mut self, render_state: &egui_wgpu::RenderState, path: &Path) {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(_) => {
                self.error_message = Some("Could not read that file.".to_string());
                return;
            }
        };
        let toolpath = parse_gcode(&source);
        if toolpath.layer_count == 0 {
            self.error_message = Some("No extrusion moves found in that file.".to_string());
            return;
        }
        let mesh = build_toolpath_mesh(&toolpath);
        render_state
            .renderer
            .write()
            .callback_resources
            .get_mut::<ToolpathGpu>()
            .expect("toolpath gpu resources registered at startup")
            .upload_mesh(&render_state.device, &mesh, toolpath.min, toolpath.max);
        self.camera.fit_bounds(toolpath.min, toolpath.max);
        self.first_layer = 0;
        self.last_layer = toolpath.layer_count - 1;
        self.error_message = None;
        self.model = Some(LoadedModel {
            file_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            toolpath,
            mesh,
        });
    }

    fn pick_file(&mut self, render_state: &egui_wgpu::RenderState) {
        let picked = rfd::FileDialog::new()
            .add_filter("G-code", &["gcode", "gco", "g", "nc"])
            .pick_file();
        if let Some(path) = picked {
            self.load_file(render_state, &path);
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context, render_state: &egui_wgpu::RenderState) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if let Some(file) = dropped.first() {
            self.load_file(render_state, file.path());
        }
    }

    fn show_side_panel(&mut self, ui: &mut egui::Ui, render_state: &egui_wgpu::RenderState) {
        let open_label = RichText::new("Open file…").strong().color(Color32::BLACK);
        let open_button = egui::Button::new(open_label).fill(ACCENT);
        if ui
            .add_sized([ui.available_width(), OPEN_BUTTON_HEIGHT], open_button)
            .clicked()
        {
            self.pick_file(render_state);
        }
        if let Some(message) = &self.error_message {
            ui.colored_label(Color32::LIGHT_RED, message);
        }
        let Some(model) = &self.model else {
            ui.label(weak_text("Open or drop a .gcode file."));
            return;
        };
        let layer_count = model.toolpath.layer_count;
        let size = model.toolpath.max - model.toolpath.min;
        section(ui, "File", |ui| {
            ui.label(heading_text(&model.file_name));
            value_row(ui, "Layers", layer_count.to_string());
            value_row(
                ui,
                "Size",
                format!("{:.1} × {:.1} × {:.1} mm", size.x, size.y, size.z),
            );
            value_row(
                ui,
                "Filament",
                format!("{:.2} m", model.toolpath.filament_mm / 1000.0),
            );
        });
        section(ui, "Layers", |ui| {
            let top_layer = layer_count - 1;
            slider_row(
                ui,
                "Top",
                format!("{} / {}", self.last_layer + 1, layer_count),
                &mut self.last_layer,
                0..=top_layer,
            );
            slider_row(
                ui,
                "Bottom",
                (self.first_layer + 1).to_string(),
                &mut self.first_layer,
                0..=top_layer,
            );
            self.first_layer = self.first_layer.min(self.last_layer);
            ui.checkbox(&mut self.show_travel, "Show travel moves");
        });
        section(ui, "Rendering", |ui| {
            show_quality_picker(ui, &mut self.quality);
        });
        section(ui, "Light", |ui| {
            ui.add_enabled_ui(self.quality != RenderQuality::Fast, |ui| {
                slider_row(
                    ui,
                    "Direction",
                    format!("{:.0}°", self.light.azimuth_degrees),
                    &mut self.light.azimuth_degrees,
                    0.0..=360.0,
                );
                slider_row(
                    ui,
                    "Height",
                    format!("{:.0}°", self.light.elevation_degrees),
                    &mut self.light.elevation_degrees,
                    MIN_LIGHT_ELEVATION_DEGREES..=MAX_LIGHT_ELEVATION_DEGREES,
                );
            });
        });
        let reset_button = egui::Button::new("Reset camera and light");
        if ui
            .add_sized([ui.available_width(), RESET_BUTTON_HEIGHT], reset_button)
            .clicked()
        {
            self.camera
                .fit_bounds(model.toolpath.min, model.toolpath.max);
            self.light = LightAngles::default();
        }
        ui.label(weak_text("Drag: orbit · Right drag: pan · Scroll: zoom").small());
    }

    fn show_viewport(&mut self, ui: &mut egui::Ui) {
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        let delta = glam::Vec2::new(response.drag_delta().x, response.drag_delta().y);
        if response.dragged_by(egui::PointerButton::Primary) {
            self.camera.orbit(delta);
        }
        if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            self.camera.pan(delta, rect.height());
        }
        if response.hovered() {
            self.camera.zoom(ui.input(|input| input.smooth_scroll_delta.y));
        }
        let Some(model) = &self.model else { return };
        let draw = ToolpathDraw {
            view_projection: self
                .camera
                .view_projection(rect.width() / rect.height().max(1.0)),
            camera_position: self.camera.eye(),
            quality: self.quality,
            light: self.light,
            extrude_range: model
                .mesh
                .extrude
                .instance_range(self.first_layer, self.last_layer),
            travel_range: self.show_travel.then(|| {
                model
                    .mesh
                    .travel
                    .vertex_range(self.first_layer, self.last_layer)
            }),
        };
        ui.painter()
            .add(egui_wgpu::Callback::new_paint_callback(rect, draw));
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let render_state = frame
            .wgpu_render_state()
            .expect("wgpu backend is required")
            .clone();
        self.handle_dropped_files(ui.ctx(), &render_state);
        egui::Panel::left("controls")
            .resizable(false)
            .exact_size(PANEL_WIDTH)
            .frame(panel_frame())
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.show_side_panel(ui, &render_state));
            });
        egui::CentralPanel::default()
            .frame(viewport_frame())
            .show(ui, |ui| self.show_viewport(ui));
    }
}

fn show_quality_picker(ui: &mut egui::Ui, quality: &mut RenderQuality) {
    let spacing = ui.spacing().item_spacing.x;
    let count = RenderQuality::ALL.len() as f32;
    let button_width = (ui.available_width() - spacing * (count - 1.0)) / count;
    ui.horizontal(|ui| {
        for option in RenderQuality::ALL {
            let button = egui::Button::selectable(*quality == option, option.short_label());
            let response = ui
                .add_sized([button_width, QUALITY_BUTTON_HEIGHT], button)
                .on_hover_text(option.description());
            if response.clicked() {
                *quality = option;
            }
        }
    });
}
