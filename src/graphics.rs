use glam::{Mat4, Quat, Vec3, Vec4};

#[cfg(all(windows, target_pointer_width = "64"))]
pub mod d3d11;
#[cfg(all(windows, target_pointer_width = "64"))]
pub(crate) mod desktop_mirror;

#[derive(Clone, Copy)]
pub struct Camera {
    pub position: Vec3,
    pub orientation: Quat,
    /// Tangents of left, right, down and up per-eye angles.
    pub fov: [f32; 4],
}

impl Camera {
    pub fn view_projection(self) -> Mat4 {
        let [l, r, b, t] = self.fov;
        let near = 0.04;
        let far = 200.0;
        let depth = far / (near - far);
        let projection = Mat4::from_cols(
            Vec4::new(2.0 / (r - l), 0.0, 0.0, 0.0),
            Vec4::new(0.0, 2.0 / (t - b), 0.0, 0.0),
            Vec4::new((r + l) / (r - l), (t + b) / (t - b), depth, -1.0),
            Vec4::new(0.0, 0.0, depth * near, 0.0),
        );
        projection * Mat4::from_rotation_translation(self.orientation, self.position).inverse()
    }
}

pub struct BoxMesh {
    pub center: Vec3,
    pub size: Vec3,
    pub orientation: Quat,
    pub color: Vec3,
    pub material: f32,
}

impl BoxMesh {
    pub fn segment(from: Vec3, to: Vec3, width: f32, color: Vec3) -> Option<Self> {
        let delta = to - from;
        let length = delta.length();
        (length > 0.001).then(|| Self {
            center: (from + to) * 0.5,
            size: Vec3::new(width, length + width * 0.6, width),
            orientation: Quat::from_rotation_arc(Vec3::Y, delta / length),
            color,
            material: 0.0,
        })
    }
}

#[derive(Default)]
pub struct Scene {
    pub boxes: Vec<BoxMesh>,
    pub spheres: Vec<SphereMesh>,
    pub cloud_density: f32,
    pub volumetric_density: f32,
    /// Enable GPU scene intersections for materials that call the reflection API.
    pub ray_tracing: bool,
    /// Number of leading scene boxes to include in ray queries.
    pub traced_boxes: usize,
    /// Extra boxes, addressed by index, that participate in reflection rays.
    pub ray_avatar_boxes: Vec<usize>,
}

pub struct SphereMesh {
    pub center: Vec3,
    pub radius: f32,
    pub color: Vec3,
    pub material: f32,
}
