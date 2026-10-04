use glam::{Mat4, Vec3};

const FIELD_OF_VIEW_RADIANS: f32 = std::f32::consts::FRAC_PI_4;
const MAX_PITCH_RADIANS: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
const ORBIT_RADIANS_PER_POINT: f32 = 0.01;
const ZOOM_PER_SCROLL_POINT: f32 = 0.002;
const MIN_DISTANCE_MM: f32 = 1.0;
const FIT_MARGIN: f32 = 1.2;
const DEFAULT_YAW_RADIANS: f32 = -0.8;
const DEFAULT_PITCH_RADIANS: f32 = 0.6;
const SNAP_SPEED_PER_SECOND: f32 = 18.0;
const SNAP_FINISH_EPSILON_RADIANS: f32 = 0.001;
const HORIZONTAL_EPSILON: f32 = 1e-4;
/// Yaw for straight-down and straight-up views: X points right, Y points up on screen.
const AXIS_ALIGNED_YAW_RADIANS: f32 = -std::f32::consts::FRAC_PI_2;

#[derive(Clone, Copy)]
struct AngleGoal {
    yaw: f32,
    pitch: f32,
}

#[derive(Clone, Copy)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub orthographic: bool,
    goal: Option<AngleGoal>,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 300.0,
            yaw: DEFAULT_YAW_RADIANS,
            pitch: DEFAULT_PITCH_RADIANS,
            orthographic: false,
            goal: None,
        }
    }
}

fn shortest_angle_difference(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

impl OrbitCamera {
    pub fn fit_bounds(&mut self, min: Vec3, max: Vec3) {
        let radius = ((max - min) * 0.5).length().max(1.0);
        self.target = (min + max) * 0.5;
        self.distance = radius * FIT_MARGIN / (FIELD_OF_VIEW_RADIANS * 0.5).tan();
        self.yaw = DEFAULT_YAW_RADIANS;
        self.pitch = DEFAULT_PITCH_RADIANS;
        self.goal = None;
    }

    pub fn orbit(&mut self, delta_points: glam::Vec2) {
        self.goal = None;
        self.yaw -= delta_points.x * ORBIT_RADIANS_PER_POINT;
        self.pitch = (self.pitch + delta_points.y * ORBIT_RADIANS_PER_POINT)
            .clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS);
    }

    pub fn step_orbit(&mut self, yaw_delta: f32, pitch_delta: f32) {
        let base = self.goal.unwrap_or(AngleGoal {
            yaw: self.yaw,
            pitch: self.pitch,
        });
        self.goal = Some(AngleGoal {
            yaw: base.yaw + yaw_delta,
            pitch: (base.pitch + pitch_delta).clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS),
        });
    }

    /// Smoothly turns the camera so it looks at the target from `direction`
    /// (a vector pointing from the target toward the eye).
    pub fn snap_to_direction(&mut self, direction: Vec3) {
        let direction = direction.normalize();
        let horizontal = direction.truncate().length();
        let yaw = if horizontal < HORIZONTAL_EPSILON {
            AXIS_ALIGNED_YAW_RADIANS
        } else {
            direction.y.atan2(direction.x)
        };
        self.goal = Some(AngleGoal {
            yaw,
            pitch: direction
                .z
                .asin()
                .clamp(-MAX_PITCH_RADIANS, MAX_PITCH_RADIANS),
        });
    }

    pub fn snap_to_opposite(&mut self) {
        self.snap_to_direction(-self.view_direction());
    }

    /// Advances a pending snap. Returns true while the camera is still moving.
    pub fn animate(&mut self, delta_seconds: f32) -> bool {
        let Some(goal) = self.goal else { return false };
        let blend = 1.0 - (-delta_seconds * SNAP_SPEED_PER_SECOND).exp();
        let yaw_difference = shortest_angle_difference(self.yaw, goal.yaw);
        let pitch_difference = goal.pitch - self.pitch;
        if yaw_difference.abs() < SNAP_FINISH_EPSILON_RADIANS
            && pitch_difference.abs() < SNAP_FINISH_EPSILON_RADIANS
        {
            self.yaw = goal.yaw;
            self.pitch = goal.pitch;
            self.goal = None;
            return false;
        }
        self.yaw += yaw_difference * blend;
        self.pitch += pitch_difference * blend;
        true
    }

    pub fn pan(&mut self, delta_points: glam::Vec2, viewport_height_points: f32) {
        let world_per_point =
            2.0 * self.distance * (FIELD_OF_VIEW_RADIANS * 0.5).tan() / viewport_height_points;
        let (right, up) = self.right_and_up();
        self.target += (-right * delta_points.x + up * delta_points.y) * world_per_point;
    }

    pub fn zoom(&mut self, scroll_points: f32) {
        self.distance =
            (self.distance * (-scroll_points * ZOOM_PER_SCROLL_POINT).exp()).max(MIN_DISTANCE_MM);
    }

    /// Unit vector from the target toward the eye.
    pub fn view_direction(&self) -> Vec3 {
        Vec3::new(
            self.pitch.cos() * self.yaw.cos(),
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
        )
    }

    /// Screen-space right and up axes expressed in world space.
    pub fn right_and_up(&self) -> (Vec3, Vec3) {
        let forward = -self.view_direction();
        let right = forward.cross(Vec3::Z).normalize();
        (right, right.cross(forward))
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.view_direction() * self.distance
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let near = (self.distance * 0.01).max(0.1);
        let far = self.distance * 20.0;
        let projection = if self.orthographic {
            let half_height = self.distance * (FIELD_OF_VIEW_RADIANS * 0.5).tan();
            let half_width = half_height * aspect;
            Mat4::orthographic_rh(-half_width, half_width, -half_height, half_height, near, far)
        } else {
            Mat4::perspective_rh(FIELD_OF_VIEW_RADIANS, aspect, near, far)
        };
        projection * Mat4::look_at_rh(self.eye(), self.target, Vec3::Z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_look_from_negative_y_for_front_view() {
        let mut camera = OrbitCamera::default();
        camera.snap_to_direction(Vec3::NEG_Y);
        while camera.animate(0.1) {}
        assert!((camera.view_direction() - Vec3::NEG_Y).length() < 1e-2);
    }

    #[test]
    fn should_align_axes_when_snapping_to_top_view() {
        let mut camera = OrbitCamera::default();
        camera.snap_to_direction(Vec3::Z);
        while camera.animate(0.1) {}
        assert!((camera.yaw - AXIS_ALIGNED_YAW_RADIANS).abs() < 1e-3);
        assert!(camera.view_direction().z > 0.99);
    }

    #[test]
    fn should_flip_to_opposite_side() {
        let mut camera = OrbitCamera::default();
        let before = camera.view_direction();
        camera.snap_to_opposite();
        while camera.animate(0.1) {}
        assert!((camera.view_direction() + before).length() < 1e-2);
    }

    #[test]
    fn should_take_shortest_path_across_the_yaw_seam() {
        let difference = shortest_angle_difference(3.0, -3.0);
        assert!(difference > 0.0 && difference < 1.0);
    }
}
