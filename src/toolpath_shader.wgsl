struct Uniforms {
    view_projection: mat4x4<f32>,
    light_view_projection: mat4x4<f32>,
    travel_color: vec4<f32>,
    light_direction: vec4<f32>,
    camera_position: vec4<f32>,
    // x: line width, y: layer height, z: plane z, w: shadow texel size
    params: vec4<f32>,
    // min x, min y, max x, max y
    plane: vec4<f32>,
    palette: array<vec4<f32>, 5>,
    type_colors: array<vec4<f32>, 12>,
    // x: colour mode (0 height, 1 speed, 2 layer, 3 feature type, 4 filament)
    color_mode: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;

const SHADOW_BIAS: f32 = 0.001;
const GAMMA: f32 = 2.2;
const PLANE_COLOR: vec3<f32> = vec3<f32>(0.20, 0.215, 0.24);

// Tube corners as (along, side, up): along 0..1, side -1..1, up -1 (bottom) .. 0 (top).
// Three quads (top, left, right), each emitted as two triangles.
var<private> QUAD_CORNERS: array<vec3<f32>, 12> = array<vec3<f32>, 12>(
    vec3<f32>(0.0, -1.0, 0.0), vec3<f32>(1.0, -1.0, 0.0), vec3<f32>(1.0, 1.0, 0.0), vec3<f32>(0.0, 1.0, 0.0),
    vec3<f32>(0.0, -1.0, -1.0), vec3<f32>(1.0, -1.0, -1.0), vec3<f32>(1.0, -1.0, 0.0), vec3<f32>(0.0, -1.0, 0.0),
    vec3<f32>(0.0, 1.0, -1.0), vec3<f32>(1.0, 1.0, -1.0), vec3<f32>(1.0, 1.0, 0.0), vec3<f32>(0.0, 1.0, 0.0),
);
var<private> QUAD_TRIANGLE_INDEX: array<u32, 6> = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
var<private> PLANE_CORNER_INDEX: array<u32, 6> = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);

struct TubeFrame {
    direction: vec3<f32>,
    side: vec3<f32>,
    up: vec3<f32>,
};

fn tube_frame(start_point: vec3<f32>, end_point: vec3<f32>) -> TubeFrame {
    let delta = end_point - start_point;
    let length_mm = length(delta);
    var direction = vec3<f32>(1.0, 0.0, 0.0);
    if (length_mm > 1e-6) {
        direction = delta / length_mm;
    }
    var side_raw = cross(direction, vec3<f32>(0.0, 0.0, 1.0));
    if (length(side_raw) < 1e-4) {
        side_raw = vec3<f32>(1.0, 0.0, 0.0);
    }
    let side = normalize(side_raw);
    return TubeFrame(direction, side, cross(side, direction));
}

fn tube_world_position(vertex_index: u32, start_point: vec3<f32>, end_point: vec3<f32>) -> vec3<f32> {
    let frame = tube_frame(start_point, end_point);
    let width = uniforms.params.x;
    let height = uniforms.params.y;
    let face = vertex_index / 6u;
    let corner = QUAD_CORNERS[face * 4u + QUAD_TRIANGLE_INDEX[vertex_index % 6u]];
    let overshoot = width * 0.5;
    let along = mix(start_point - frame.direction * overshoot, end_point + frame.direction * overshoot, corner.x);
    return along + frame.side * (corner.y * width * 0.5) + frame.up * (corner.z * height);
}

fn tube_normal(vertex_index: u32, start_point: vec3<f32>, end_point: vec3<f32>) -> vec3<f32> {
    let frame = tube_frame(start_point, end_point);
    let face = vertex_index / 6u;
    if (face == 0u) {
        return frame.up;
    }
    if (face == 1u) {
        return -frame.side;
    }
    return frame.side;
}

fn shadow_factor(world_position: vec3<f32>) -> f32 {
    if (uniforms.light_direction.w < 0.5) {
        return 1.0;
    }
    let light_clip = uniforms.light_view_projection * vec4<f32>(world_position, 1.0);
    let ndc = light_clip.xyz / light_clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0) {
        return 1.0;
    }
    let texel = uniforms.params.w;
    var lit = 0.0;
    for (var dx = -1; dx <= 1; dx++) {
        for (var dy = -1; dy <= 1; dy++) {
            let offset = vec2<f32>(f32(dx), f32(dy)) * texel;
            lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, ndc.z - SHADOW_BIAS);
        }
    }
    return lit / 9.0;
}

fn light_surface(base_srgb: vec3<f32>, normal: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    let base = pow(base_srgb, vec3<f32>(GAMMA));
    let n = normalize(normal);
    let light = normalize(uniforms.light_direction.xyz);
    let view = normalize(uniforms.camera_position.xyz - world_position);
    let sky = mix(0.22, 0.55, n.z * 0.5 + 0.5);
    let key = max(dot(n, light), 0.0) * shadow_factor(world_position) * 0.85;
    let fill = max(dot(n, view), 0.0) * 0.12;
    let lit = base * (sky + key + fill);
    return pow(lit, vec3<f32>(1.0 / GAMMA));
}

fn palette_color(t: f32) -> vec3<f32> {
    let scaled = clamp(t, 0.0, 1.0) * 4.0;
    let index = min(u32(scaled), 3u);
    return mix(uniforms.palette[index].rgb, uniforms.palette[index + 1u].rgb, scaled - f32(index));
}

// values: x height, y speed, z layer (all 0..1), w feature index + 16 * tool index
fn extrude_color(values: vec4<f32>) -> vec3<f32> {
    let mode = u32(uniforms.color_mode.x);
    let packed = u32(values.w);
    if (mode == 3u) {
        return uniforms.type_colors[packed % 16u].rgb;
    }
    if (mode == 4u) {
        return uniforms.type_colors[packed / 16u].rgb;
    }
    return palette_color(values[mode]);
}

struct SurfaceVertex {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) base_color: vec3<f32>,
};

@vertex
fn vs_tube(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) start_point: vec3<f32>,
    @location(1) end_point: vec3<f32>,
    @location(2) values: vec4<f32>,
) -> SurfaceVertex {
    let world = tube_world_position(vertex_index, start_point, end_point);
    var output: SurfaceVertex;
    output.clip_position = uniforms.view_projection * vec4<f32>(world, 1.0);
    output.world_position = world;
    output.normal = tube_normal(vertex_index, start_point, end_point);
    output.base_color = extrude_color(values);
    return output;
}

@fragment
fn fs_tube(input: SurfaceVertex) -> @location(0) vec4<f32> {
    return vec4<f32>(light_surface(input.base_color, input.normal, input.world_position), 1.0);
}

@vertex
fn vs_shadow(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) start_point: vec3<f32>,
    @location(1) end_point: vec3<f32>,
    @location(2) values: vec4<f32>,
) -> @builtin(position) vec4<f32> {
    let world = tube_world_position(vertex_index, start_point, end_point);
    return uniforms.light_view_projection * vec4<f32>(world, 1.0);
}

@vertex
fn vs_plane(@builtin(vertex_index) vertex_index: u32) -> SurfaceVertex {
    let corners = array<vec2<f32>, 4>(
        vec2<f32>(uniforms.plane.x, uniforms.plane.y),
        vec2<f32>(uniforms.plane.z, uniforms.plane.y),
        vec2<f32>(uniforms.plane.z, uniforms.plane.w),
        vec2<f32>(uniforms.plane.x, uniforms.plane.w),
    );
    let xy = corners[PLANE_CORNER_INDEX[vertex_index]];
    let world = vec3<f32>(xy, uniforms.params.z);
    var output: SurfaceVertex;
    output.clip_position = uniforms.view_projection * vec4<f32>(world, 1.0);
    output.world_position = world;
    output.normal = vec3<f32>(0.0, 0.0, 1.0);
    output.base_color = vec3<f32>(0.0);
    return output;
}

@fragment
fn fs_plane(input: SurfaceVertex) -> @location(0) vec4<f32> {
    let center = (uniforms.plane.xy + uniforms.plane.zw) * 0.5;
    let half_extent = (uniforms.plane.zw - uniforms.plane.xy) * 0.5;
    let edge_distance = length((input.world_position.xy - center) / half_extent);
    let fade = 1.0 - smoothstep(0.55, 1.0, edge_distance);
    let color = light_surface(PLANE_COLOR, input.normal, input.world_position);
    return vec4<f32>(color, fade);
}

struct LineVertex {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex
fn vs_line(@location(0) position: vec3<f32>, @location(1) height: f32) -> LineVertex {
    var output: LineVertex;
    output.clip_position = uniforms.view_projection * vec4<f32>(position, 1.0);
    return output;
}

@fragment
fn fs_line(input: LineVertex) -> @location(0) vec4<f32> {
    return uniforms.travel_color;
}

struct ExtrudeLineVertex {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_extrude_line(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) start_point: vec3<f32>,
    @location(1) end_point: vec3<f32>,
    @location(2) values: vec4<f32>,
) -> ExtrudeLineVertex {
    let position = select(start_point, end_point, vertex_index == 1u);
    var output: ExtrudeLineVertex;
    output.clip_position = uniforms.view_projection * vec4<f32>(position, 1.0);
    output.color = extrude_color(values);
    return output;
}

@fragment
fn fs_extrude_line(input: ExtrudeLineVertex) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
