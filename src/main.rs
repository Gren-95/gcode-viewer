mod app_theme;
mod orbit_camera;
mod parse_gcode;
mod toolpath_mesh;
mod toolpath_renderer;
mod viewer_app;

use std::path::PathBuf;
use viewer_app::ViewerApp;

fn main() -> eframe::Result {
    let initial_file = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Gcode Viewer")
            .with_app_id("gcode-viewer")
            .with_inner_size([1200.0, 800.0]),
        renderer: eframe::Renderer::Wgpu,
        depth_buffer: toolpath_renderer::DEPTH_BUFFER_BITS,
        multisampling: toolpath_renderer::MSAA_SAMPLES as u16,
        ..Default::default()
    };
    eframe::run_native(
        "gcode-viewer",
        options,
        Box::new(|cc| Ok(Box::new(ViewerApp::new(cc, initial_file)))),
    )
}
