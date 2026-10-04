use crate::orbit_camera::OrbitCamera;
use crate::parse_gcode::{Toolpath, parse_gcode};
use crate::toolpath_mesh::{ToolpathMesh, build_toolpath_mesh};
use crate::toolpath_renderer::{RenderQuality, ToolpathDraw, ToolpathGpu};
use eframe::egui::{self, Color32, RichText};
use eframe::egui_wgpu;
use std::path::{Path, PathBuf};

const BACKGROUND_COLOR: Color32 = Color32::from_rgb(19, 21, 25);

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
    error_message: Option<String>,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
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
        if ui.button("Open file…").clicked() {
            self.pick_file(render_state);
        }
        if let Some(message) = &self.error_message {
            ui.colored_label(Color32::LIGHT_RED, message);
        }
        let Some(model) = &self.model else {
            ui.label("Open or drop a .gcode file.");
            return;
        };
        ui.separator();
        ui.label(RichText::new(&model.file_name).strong());
        let size = model.toolpath.max - model.toolpath.min;
        ui.label(format!("Layers: {}", model.toolpath.layer_count));
        ui.label(format!(
            "Size: {:.1} × {:.1} × {:.1} mm",
            size.x, size.y, size.z
        ));
        ui.label(format!("Filament: {:.2} m", model.toolpath.filament_mm / 1000.0));
        ui.separator();
        let top_layer = model.toolpath.layer_count - 1;
        ui.label("Layer range");
        ui.add(egui::Slider::new(&mut self.last_layer, 0..=top_layer).text("top"));
        ui.add(egui::Slider::new(&mut self.first_layer, 0..=top_layer).text("bottom"));
        self.first_layer = self.first_layer.min(self.last_layer);
        ui.checkbox(&mut self.show_travel, "Show travel moves");
        ui.separator();
        ui.label("Rendering");
        for quality in RenderQuality::ALL {
            ui.radio_value(&mut self.quality, quality, quality.label());
        }
        if ui.button("Reset camera").clicked() {
            self.camera
                .fit_bounds(model.toolpath.min, model.toolpath.max);
        }
        ui.separator();
        ui.small("Left drag: orbit\nRight/middle drag: pan\nScroll: zoom");
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
            .show(ui, |ui| self.show_side_panel(ui, &render_state));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BACKGROUND_COLOR))
            .show(ui, |ui| self.show_viewport(ui));
    }
}
