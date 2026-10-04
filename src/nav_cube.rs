use crate::app_theme::ACCENT;
use crate::orbit_camera::OrbitCamera;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, vec2};
use glam::Vec3;

const CUBE_AREA_POINTS: f32 = 120.0;
const CUBE_MARGIN_POINTS: f32 = 16.0;
const CUBE_SCALE_POINTS: f32 = 32.0;
const CELLS_PER_AXIS: i32 = 3;
const FACE_FACING_EPSILON: f32 = 0.01;
const FACE_FILL: Color32 = Color32::from_rgb(52, 57, 69);
const FACE_STROKE: Color32 = Color32::from_rgb(92, 99, 116);
const LABEL_COLOR: Color32 = Color32::from_rgb(210, 214, 224);
const HOVER_FILL_ALPHA: u8 = 200;

struct Face {
    label: &'static str,
    normal: Vec3,
    tangent_u: Vec3,
    tangent_v: Vec3,
}

const FACES: [Face; 6] = [
    Face {
        label: "FRONT",
        normal: Vec3::NEG_Y,
        tangent_u: Vec3::X,
        tangent_v: Vec3::Z,
    },
    Face {
        label: "BACK",
        normal: Vec3::Y,
        tangent_u: Vec3::NEG_X,
        tangent_v: Vec3::Z,
    },
    Face {
        label: "RIGHT",
        normal: Vec3::X,
        tangent_u: Vec3::Y,
        tangent_v: Vec3::Z,
    },
    Face {
        label: "LEFT",
        normal: Vec3::NEG_X,
        tangent_u: Vec3::NEG_Y,
        tangent_v: Vec3::Z,
    },
    Face {
        label: "TOP",
        normal: Vec3::Z,
        tangent_u: Vec3::X,
        tangent_v: Vec3::Y,
    },
    Face {
        label: "BOTTOM",
        normal: Vec3::NEG_Z,
        tangent_u: Vec3::X,
        tangent_v: Vec3::NEG_Y,
    },
];

pub fn nav_cube_rect(viewport: Rect) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            viewport.right() - CUBE_AREA_POINTS - CUBE_MARGIN_POINTS,
            viewport.top() + CUBE_MARGIN_POINTS,
        ),
        vec2(CUBE_AREA_POINTS, CUBE_AREA_POINTS),
    )
}

struct Projection {
    center: Pos2,
    right: Vec3,
    up: Vec3,
}

impl Projection {
    fn project(&self, point: Vec3) -> Pos2 {
        self.center
            + vec2(
                point.dot(self.right) * CUBE_SCALE_POINTS,
                -point.dot(self.up) * CUBE_SCALE_POINTS,
            )
    }
}

fn cell_offsets() -> impl Iterator<Item = (i32, i32)> {
    let half = CELLS_PER_AXIS / 2;
    (-half..=half).flat_map(move |u| (-half..=half).map(move |v| (u, v)))
}

fn cell_corners(face: &Face, cell: (i32, i32), projection: &Projection) -> Vec<Pos2> {
    let step = 2.0 / CELLS_PER_AXIS as f32;
    let center_u = cell.0 as f32 * step;
    let center_v = cell.1 as f32 * step;
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|(du, dv)| {
            let u = center_u + du * step * 0.5;
            let v = center_v + dv * step * 0.5;
            projection.project(face.normal + face.tangent_u * u + face.tangent_v * v)
        })
        .collect()
}

fn polygon_contains(points: &[Pos2], target: Pos2) -> bool {
    let mut inside = false;
    let mut previous = points[points.len() - 1];
    for &current in points {
        let crosses = (current.y > target.y) != (previous.y > target.y);
        if crosses
            && target.x
                < (previous.x - current.x) * (target.y - current.y) / (previous.y - current.y)
                    + current.x
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn cell_direction(face: &Face, cell: (i32, i32)) -> Vec3 {
    (face.normal
        + face.tangent_u * cell.0.signum() as f32
        + face.tangent_v * cell.1.signum() as f32)
        .normalize()
}

/// Draws the navigation cube and returns the view direction of a clicked face, edge or corner.
pub fn show_nav_cube(ui: &mut egui::Ui, viewport: Rect, camera: &OrbitCamera) -> Option<Vec3> {
    let area = nav_cube_rect(viewport);
    let response = ui.interact(area, ui.id().with("nav_cube"), Sense::click_and_drag());
    let (right, up) = camera.right_and_up();
    let projection = Projection {
        center: area.center(),
        right,
        up,
    };
    let toward_viewer = camera.view_direction();
    let hover = response.hover_pos();
    let painter = ui.painter();
    let mut clicked_direction = None;
    for face in FACES
        .iter()
        .filter(|face| face.normal.dot(toward_viewer) > FACE_FACING_EPSILON)
    {
        let full: Vec<Pos2> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .iter()
            .map(|(u, v)| {
                projection.project(face.normal + face.tangent_u * *u + face.tangent_v * *v)
            })
            .collect();
        painter.add(Shape::convex_polygon(
            full.clone(),
            FACE_FILL,
            Stroke::new(1.0, FACE_STROKE),
        ));
        if let Some(cell) = hover.and_then(|hover| {
            cell_offsets().find(|&cell| polygon_contains(&cell_corners(face, cell, &projection), hover))
        }) {
            let hovered_fill = Color32::from_rgba_unmultiplied(
                ACCENT.r(),
                ACCENT.g(),
                ACCENT.b(),
                HOVER_FILL_ALPHA,
            );
            painter.add(Shape::convex_polygon(
                cell_corners(face, cell, &projection),
                hovered_fill,
                Stroke::NONE,
            ));
            if response.clicked() {
                clicked_direction = Some(cell_direction(face, cell));
            }
        }
        let face_center = projection.project(face.normal);
        painter.text(
            face_center,
            egui::Align2::CENTER_CENTER,
            face.label,
            egui::FontId::proportional(9.0),
            LABEL_COLOR,
        );
    }
    clicked_direction
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_detect_points_inside_polygon() {
        let square = [
            Pos2::new(0.0, 0.0),
            Pos2::new(10.0, 0.0),
            Pos2::new(10.0, 10.0),
            Pos2::new(0.0, 10.0),
        ];
        assert!(polygon_contains(&square, Pos2::new(5.0, 5.0)));
        assert!(!polygon_contains(&square, Pos2::new(15.0, 5.0)));
    }

    #[test]
    fn should_snap_center_cell_to_face_normal() {
        let top = &FACES[4];
        assert!((cell_direction(top, (0, 0)) - Vec3::Z).length() < 1e-6);
    }

    #[test]
    fn should_snap_edge_and_corner_cells_to_diagonals() {
        let top = &FACES[4];
        let edge = cell_direction(top, (1, 0));
        assert!((edge - (Vec3::Z + Vec3::X).normalize()).length() < 1e-6);
        let corner = cell_direction(top, (1, 1));
        assert!((corner - (Vec3::Z + Vec3::X + Vec3::Y).normalize()).length() < 1e-6);
    }

    #[test]
    fn should_cover_every_cell_of_a_face() {
        assert_eq!(cell_offsets().count(), 9);
    }
}
