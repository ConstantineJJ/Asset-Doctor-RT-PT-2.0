use ad_document::Bounds;
use glam::{Vec3, camera::rh};

pub struct Camera {
    center: Vec3,
    radius: f32,
    distance: f32,
    yaw: f32,
    pitch: f32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Vec3::ZERO,
            radius: 1.0,
            distance: 4.0,
            yaw: 0.7,
            pitch: 0.35,
        }
    }
}
impl Camera {
    pub fn fit(&mut self, bounds: Bounds) {
        let min = Vec3::from_array(bounds.min);
        let max = Vec3::from_array(bounds.max);
        self.center = min * 0.5 + max * 0.5;
        self.radius = ((max - min).length() * 0.5)
            .max(self.center.abs().max_element() * f32::EPSILON * 8.0)
            .max(0.001);
        self.distance = self.radius * 3.2;
    }
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw = (self.yaw - dx * 0.005) % std::f32::consts::TAU;
        self.pitch = (self.pitch + dy * 0.005).clamp(-1.5, 1.5);
    }
    fn direction(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
    }
    pub fn pan(&mut self, dx: f32, dy: f32, height: f32) {
        let right = Vec3::new(self.yaw.cos(), 0., -self.yaw.sin());
        let up = self.direction().cross(right);
        self.center += (-right * dx + up * dy) * (self.distance * 0.828 / height.max(1.0));
    }
    pub fn zoom(&mut self, steps: f32) {
        self.distance =
            (self.distance * (-steps * 0.15).exp()).clamp(self.radius * 0.05, self.radius * 1000.0);
    }
    pub fn matrix(&self, width: u32, height: u32) -> [[f32; 4]; 4] {
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let distance = self.distance / aspect.clamp(0.05, 1.0);
        let eye = self.center + self.direction() * distance;
        let near = (self.radius * 0.0001).max(0.000001);
        let far = (distance + self.radius * 4.0)
            .max(self.radius * 10.0)
            .max(near * 100.0);
        let projection = rh::proj::vulkan::perspective(45f32.to_radians(), aspect, near, far);
        (projection * rh::view::look_at_mat4(eye, self.center, Vec3::Y)).to_cols_array_2d()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_fit_orbit_pan_zoom_stay_finite_for_small_and_large_bounds() {
        for size in [0., 0.0001, 1., 1e9] {
            let mut camera = Camera::default();
            let bounds = Bounds {
                min: [-size; 3],
                max: [size; 3],
            };
            camera.fit(bounds);
            let initial = camera.matrix(1200, 760);
            camera.orbit(80., -40.);
            camera.pan(5., 4., 760.);
            camera.zoom(3.);
            let moved = camera.matrix(960, 600);
            assert_ne!(initial, moved);
            assert!(moved.iter().flatten().all(|v| v.is_finite()));
            assert!(camera.matrix(0, 0).iter().flatten().all(|v| v.is_finite()));
        }
    }
}
