use glam::{Mat4, Vec3};

const FIELD_OF_VIEW_RADIANS: f32 = std::f32::consts::FRAC_PI_4;
const MAX_PITCH_RADIANS: f32 = 1.55;
const ORBIT_RADIANS_PER_POINT: f32 = 0.01;
const ZOOM_PER_SCROLL_POINT: f32 = 0.002;
const MIN_DISTANCE_MM: f32 = 1.0;
const FIT_MARGIN: f32 = 0.85;
const DEFAULT_YAW_RADIANS: f32 = -0.8;
const DEFAULT_PITCH_RADIANS: f32 = 0.6;

#[derive(Clone, Copy)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 300.0,
            yaw: DEFAULT_YAW_RADIANS,
            pitch: DEFAULT_PITCH_RADIANS,
        }
    }
}

impl OrbitCamera {
    pub fn fit_bounds(&mut self, min: Vec3, max: Vec3) {
        let radius = ((max - min) * 0.5).length().max(1.0);
        self.target = (min + max) * 0.5;
        self.distance = radius * FIT_MARGIN / (FIELD_OF_VIEW_RADIANS * 0.5).tan();
        self.yaw = DEFAULT_YAW_RADIANS;
        self.pitch = DEFAULT_PITCH_RADIANS;
    }

    pub fn orbit(&mut self, delta_points: glam::Vec2) {
        self.yaw -= delta_points.x * ORBIT_RADIANS_PER_POINT;
        self.pitch = (self.pitch + delta_points.y * ORBIT_RADIANS_PER_POINT)
            .clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS);
    }

    pub fn pan(&mut self, delta_points: glam::Vec2, viewport_height_points: f32) {
        let world_per_point =
            2.0 * self.distance * (FIELD_OF_VIEW_RADIANS * 0.5).tan() / viewport_height_points;
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(Vec3::Z).normalize();
        let up = right.cross(forward);
        self.target += (-right * delta_points.x + up * delta_points.y) * world_per_point;
    }

    pub fn zoom(&mut self, scroll_points: f32) {
        self.distance =
            (self.distance * (-scroll_points * ZOOM_PER_SCROLL_POINT).exp()).max(MIN_DISTANCE_MM);
    }

    pub fn eye(&self) -> Vec3 {
        let horizontal = self.distance * self.pitch.cos();
        self.target
            + Vec3::new(
                horizontal * self.yaw.cos(),
                horizontal * self.yaw.sin(),
                self.distance * self.pitch.sin(),
            )
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let near = (self.distance * 0.01).max(0.1);
        let far = self.distance * 20.0;
        let projection = Mat4::perspective_rh(FIELD_OF_VIEW_RADIANS, aspect, near, far);
        projection * Mat4::look_at_rh(self.eye(), self.target, Vec3::Z)
    }
}
