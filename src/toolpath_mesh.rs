use crate::parse_gcode::{MoveKind, Toolpath};
use bytemuck::{Pod, Zeroable};
use std::ops::Range;

const DEFAULT_LAYER_HEIGHT_MM: f32 = 0.2;
const MIN_LAYER_HEIGHT_MM: f32 = 0.05;
const MAX_LAYER_HEIGHT_MM: f32 = 0.8;
pub const LINE_WIDTH_MM: f32 = 0.45;
/// The fourth instance value packs `feature + tool * TOOL_STRIDE`.
const TOOL_STRIDE: f32 = 16.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ExtrudeInstance {
    pub from: [f32; 3],
    pub to: [f32; 3],
    /// Normalised height, speed and layer in 0..1, then feature index plus tool index times 16.
    pub values: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct TravelVertex {
    pub position: [f32; 3],
    pub height: f32,
}

#[derive(Default)]
struct LayerOffsets(Vec<u32>);

impl LayerOffsets {
    fn record(&mut self, layer: u32, items_so_far: usize) {
        while self.0.len() <= layer as usize {
            self.0.push(items_so_far as u32);
        }
    }

    fn range(&self, first_layer: u32, last_layer: u32, total_items: usize) -> Range<u32> {
        let offset = |layer: u32| {
            self.0
                .get(layer as usize)
                .copied()
                .unwrap_or(total_items as u32)
        };
        offset(first_layer)..offset(last_layer + 1)
    }
}

#[derive(Default)]
pub struct ExtrudeMesh {
    pub instances: Vec<ExtrudeInstance>,
    layers: LayerOffsets,
}

impl ExtrudeMesh {
    pub fn instance_range(&self, first_layer: u32, last_layer: u32) -> Range<u32> {
        self.layers
            .range(first_layer, last_layer, self.instances.len())
    }
}

#[derive(Default)]
pub struct TravelMesh {
    pub vertices: Vec<TravelVertex>,
    layers: LayerOffsets,
}

impl TravelMesh {
    pub fn vertex_range(&self, first_layer: u32, last_layer: u32) -> Range<u32> {
        self.layers
            .range(first_layer, last_layer, self.vertices.len())
    }
}

#[derive(Default)]
pub struct ToolpathMesh {
    pub extrude: ExtrudeMesh,
    pub travel: TravelMesh,
    pub layer_height: f32,
}

fn estimate_layer_height(toolpath: &Toolpath) -> f32 {
    if toolpath.layer_count < 2 {
        return DEFAULT_LAYER_HEIGHT_MM;
    }
    let average = (toolpath.max.z - toolpath.min.z) / (toolpath.layer_count - 1) as f32;
    average.clamp(MIN_LAYER_HEIGHT_MM, MAX_LAYER_HEIGHT_MM)
}

pub fn build_toolpath_mesh(toolpath: &Toolpath) -> ToolpathMesh {
    let height_span = (toolpath.max.z - toolpath.min.z).max(f32::EPSILON);
    let speed_span = (toolpath.speed_max - toolpath.speed_min).max(f32::EPSILON);
    let layer_span = toolpath.layer_count.saturating_sub(1).max(1) as f32;
    let normalized_height =
        |z: f32| ((z - toolpath.min.z) / height_span).clamp(0.0, 1.0);
    let mut mesh = ToolpathMesh {
        layer_height: estimate_layer_height(toolpath),
        ..Default::default()
    };
    for segment in &toolpath.segments {
        match segment.kind {
            MoveKind::Extrude => {
                mesh.extrude
                    .layers
                    .record(segment.layer, mesh.extrude.instances.len());
                mesh.extrude.instances.push(ExtrudeInstance {
                    from: segment.from.to_array(),
                    to: segment.to.to_array(),
                    values: [
                        normalized_height(segment.to.z),
                        ((segment.speed - toolpath.speed_min) / speed_span).clamp(0.0, 1.0),
                        (segment.layer as f32 / layer_span).clamp(0.0, 1.0),
                        f32::from(segment.feature) + f32::from(segment.tool) * TOOL_STRIDE,
                    ],
                });
            }
            MoveKind::Travel => {
                mesh.travel
                    .layers
                    .record(segment.layer, mesh.travel.vertices.len());
                mesh.travel.vertices.extend([
                    TravelVertex {
                        position: segment.from.to_array(),
                        height: normalized_height(segment.from.z),
                    },
                    TravelVertex {
                        position: segment.to.to_array(),
                        height: normalized_height(segment.to.z),
                    },
                ]);
            }
        }
    }
    let last_layer = toolpath.layer_count.saturating_sub(1);
    mesh.extrude
        .layers
        .record(last_layer + 1, mesh.extrude.instances.len());
    mesh.travel
        .layers
        .record(last_layer + 1, mesh.travel.vertices.len());
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_gcode::parse_gcode;

    #[test]
    fn should_return_instance_range_for_layer_span() {
        let toolpath = parse_gcode("G1 Z0.2\nG1 X10 E1\nG1 Z0.4\nG1 X0 E2\nG1 Z0.6\nG1 X10 E3\n");
        let mesh = build_toolpath_mesh(&toolpath);
        assert_eq!(mesh.extrude.instance_range(0, 2), 0..3);
        assert_eq!(mesh.extrude.instance_range(1, 1), 1..2);
    }

    #[test]
    fn should_estimate_layer_height_from_z_span() {
        let toolpath = parse_gcode("G1 Z0.2\nG1 X10 E1\nG1 Z0.4\nG1 X0 E2\nG1 Z0.6\nG1 X10 E3\n");
        let mesh = build_toolpath_mesh(&toolpath);
        assert!((mesh.layer_height - 0.2).abs() < 1e-4);
    }
}
