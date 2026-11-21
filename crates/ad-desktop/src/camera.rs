use ad_document::Bounds;
use glam::{Vec3, camera::rh};

/// Fixed camera views: six sides (orthographic) and three quarters
/// (perspective, the starting view).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    ThreeQuarter,
}
impl View {
    pub const ALL: [Self; 7] = [
        Self::Front,
        Self::Back,
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
        Self::ThreeQuarter,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Front => "Front",
            Self::Back => "Back",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::ThreeQuarter => "3/4",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Front => "Спереди (+Z), ортографическая",
            Self::Back => "Сзади (-Z), ортографическая",
            Self::Left => "Слева (-X), ортографическая",
            Self::Right => "Справа (+X), ортографическая",
            Self::Top => "Сверху (+Y), ортографическая",
            Self::Bottom => "Снизу (-Y), ортографическая",
            Self::ThreeQuarter => "Три четверти, перспектива (как при открытии)",
        }
    }
    /// (yaw, pitch, orthographic).
    fn angles(self) -> (f32, f32, bool) {
        use std::f32::consts::{FRAC_PI_2, PI};
        match self {
            Self::Front => (0., 0., true),
            Self::Back => (PI, 0., true),
            Self::Left => (-FRAC_PI_2, 0., true),
            Self::Right => (FRAC_PI_2, 0., true),
            Self::Top => (0., FRAC_PI_2, true),
            Self::Bottom => (0., -FRAC_PI_2, true),
            Self::ThreeQuarter => (0.7, 0.35, false),
        }
    }
}

pub struct Camera {
    center: Vec3,
    radius: f32,
    distance: f32,
    yaw: f32,
    pitch: f32,
    /// Orthographic projection (fixed side views); orbiting turns it off.
    ortho: bool,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Vec3::ZERO,
            radius: 1.0,
            distance: 4.0,
            yaw: 0.7,
            pitch: 0.35,
            ortho: false,
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
        self.ortho = false;
        self.yaw = (self.yaw - dx * 0.005) % std::f32::consts::TAU;
        self.pitch = (self.pitch + dy * 0.005).clamp(-1.5, 1.5);
    }
    /// A fixed view around the current centre and zoom.
    pub fn set_view(&mut self, view: View) {
        let (yaw, pitch, ortho) = view.angles();
        self.yaw = yaw;
        self.pitch = pitch;
        self.ortho = ortho;
    }
    pub fn is_ortho(&self) -> bool {
        self.ortho
    }
    /// Up vector of the view; straight up or down looks with -Z / +Z up.
    fn up(&self) -> Vec3 {
        if self.pitch.cos() > 1e-3 {
            Vec3::Y
        } else {
            -Vec3::new(self.yaw.sin(), 0., self.yaw.cos()) * self.pitch.signum()
        }
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
        let up = if self.pitch.cos() > 1e-3 {
            self.direction().cross(right)
        } else {
            self.up()
        };
        self.center += (-right * dx + up * dy) * (self.distance * 0.828 / height.max(1.0));
    }
    pub fn zoom(&mut self, steps: f32) {
        self.distance =
            (self.distance * (-steps * 0.15).exp()).clamp(self.radius * 0.05, self.radius * 1000.0);
    }
    pub fn matrix(&self, width: u32, height: u32) -> [[f32; 4]; 4] {
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let distance = self.distance / aspect.clamp(0.05, 1.0);
        let eye = Vec3::from_array(self.eye(width, height));
        // Near plane scales with viewing distance (as Legacy does with dist/500),
        // so zooming out does not waste depth precision on empty space.
        let near = (self.radius * 0.0001).max(distance / 500.).max(0.000001);
        let far = (distance + self.radius * 4.0)
            .max(self.radius * 10.0)
            .max(near * 100.0);
        let projection = if self.ortho {
            // Same size at the centre as the perspective view would show.
            let half_h = distance * 22.5f32.to_radians().tan();
            let half_w = half_h * aspect;
            let depth = (eye - self.center).length();
            rh::proj::vulkan::orthographic(
                -half_w,
                half_w,
                -half_h,
                half_h,
                (depth - self.radius * 4.).max(near),
                depth + self.radius * 4.,
            )
        } else {
            rh::proj::vulkan::perspective(45f32.to_radians(), aspect, near, far)
        };
        // Reversed-Z (z' = w - z): near → 1, far → 0. With the D32 float depth
        // buffer this spreads precision evenly over distance, so close layers
        // (decals) do not z-fight when zoomed out. The renderer clears depth to
        // 0 and keeps the GREATER fragment (ad_render_vulkan::REVERSED_Z).
        let reverse = glam::Mat4::from_cols_array_2d(&[
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., -1., 0.],
            [0., 0., 1., 1.],
        ]);
        (reverse * projection * rh::view::look_at_mat4(eye, self.center, self.up()))
            .to_cols_array_2d()
    }
    #[cfg(test)]
    /// ADR-0040 lamp gizmo: screen position (0..1, y down) of the point
    /// `distance` model radii from the centre along `direction`; None when it
    /// is behind the camera.
    pub fn project_direction(
        &self,
        direction: [f32; 3],
        distance: f32,
        width: u32,
        height: u32,
    ) -> Option<[f32; 2]> {
        self.project(
            (self.center + Vec3::from_array(direction) * self.radius * distance).to_array(),
            width,
            height,
        )
    }
    /// Screen position (0..1, y down) of a world point; None behind the camera.
    pub fn project(&self, point: [f32; 3], width: u32, height: u32) -> Option<[f32; 2]> {
        let m = glam::Mat4::from_cols_array_2d(&self.matrix(width, height));
        let clip = m * Vec3::from_array(point).extend(1.);
        if !clip.is_finite() || clip.w <= 1e-6 {
            return None;
        }
        Some([clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5])
    }
    /// ADR-0042 helpers: a world segment on screen (0..1, y down), clipped
    /// at the near plane; None when it is entirely behind the camera.
    pub fn project_segment(
        &self,
        a: [f32; 3],
        b: [f32; 3],
        width: u32,
        height: u32,
    ) -> Option<([f32; 2], [f32; 2])> {
        let m = glam::Mat4::from_cols_array_2d(&self.matrix(width, height));
        let (mut p, mut q) = (
            m * Vec3::from_array(a).extend(1.),
            m * Vec3::from_array(b).extend(1.),
        );
        const NEAR: f32 = 1e-5;
        if !p.is_finite() || !q.is_finite() || (p.w <= NEAR && q.w <= NEAR) {
            return None;
        }
        if p.w <= NEAR {
            p = q + (p - q) * ((q.w - NEAR) / (q.w - p.w));
        } else if q.w <= NEAR {
            q = p + (q - p) * ((p.w - NEAR) / (p.w - q.w));
        }
        let screen = |c: glam::Vec4| [c.x / c.w * 0.5 + 0.5, c.y / c.w * 0.5 + 0.5];
        Some((screen(p), screen(q)))
    }
    /// Unit direction from the model centre towards the eye.
    pub fn view_direction(&self) -> [f32; 3] {
        self.direction().to_array()
    }
    pub fn eye(&self, width: u32, height: u32) -> [f32; 3] {
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let distance = self.distance / aspect.clamp(0.05, 1.0);
        // Orthographic views stand back far enough to see the whole model.
        let distance = if self.ortho {
            distance.max(self.radius * 4.)
        } else {
            distance
        };
        (self.center + self.direction() * distance).to_array()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Owner report 2026-11-19: layered surfaces (eye/blush/apron decals)
    /// flickered when zoomed out. With reversed-Z on the D32 float buffer,
    /// surfaces 1e-4 radius apart far from the camera keep distinct depths,
    /// and nearer surfaces get the larger depth (compare GREATER).
    #[test]
    fn reversed_depth_keeps_close_layers_apart_when_zoomed_out() {
        let mut camera = Camera::default();
        camera.fit(Bounds {
            min: [-1.; 3],
            max: [1.; 3],
        });
        camera.zoom(-14.); // about 20 radii away
        let m = glam::Mat4::from_cols_array_2d(&camera.matrix(1200, 760));
        let eye = Vec3::from_array(camera.eye(1200, 760));
        let toward_eye = (eye - camera.center).normalize();
        let depth = |p: Vec3| {
            let c = m * p.extend(1.);
            c.z / c.w
        };
        let back = depth(camera.center);
        let front = depth(camera.center + toward_eye * (camera.radius * 1e-4));
        assert!(front > back, "nearer layer must win: {front} vs {back}");
        assert!((0.0..=1.0).contains(&back) && (0.0..=1.0).contains(&front));
        // The eye-side end of the view volume maps to 1, the far end towards 0.
        // The near plane is distance / 500 in front of the eye.
        let near = (eye - camera.center).length() / 500.;
        assert!(depth(eye - toward_eye * (near * 1.0001)) > 0.99);
    }
    #[test]
    fn segments_are_clipped_at_the_near_plane() {
        let mut camera = Camera::default();
        camera.fit(Bounds {
            min: [-1.; 3],
            max: [1.; 3],
        });
        let eye = Vec3::from_array(camera.eye(1200, 760));
        let behind = (eye + (eye - camera.center)).to_array();
        let (p, q) = camera
            .project_segment([0.; 3], behind, 1200, 760)
            .expect("front part stays");
        assert!(p.iter().chain(q.iter()).all(|v| v.is_finite()));
        assert_eq!(p, camera.project([0.; 3], 1200, 760).unwrap());
        let further = (eye + (eye - camera.center) * 2.).to_array();
        assert!(camera.project_segment(behind, further, 1200, 760).is_none());
    }
    #[test]
    fn projection_puts_up_at_the_top_and_hides_points_behind_the_eye() {
        let mut camera = Camera::default();
        camera.fit(Bounds {
            min: [-1.; 3],
            max: [1.; 3],
        });
        let center = camera.project([0.; 3], 1200, 760).unwrap();
        assert!((center[0] - 0.5).abs() < 1e-4 && (center[1] - 0.5).abs() < 1e-4);
        let above = camera.project([0., 0.5, 0.], 1200, 760).unwrap();
        assert!(above[1] < center[1], "y grows downwards");
        let eye = Vec3::from_array(camera.eye(1200, 760));
        let behind = eye + (eye - camera.center);
        assert!(camera.project(behind.to_array(), 1200, 760).is_none());
    }
    /// ADR-0040 sky: the shader unprojects the pixel at depth 1 (near) and 0
    /// (far); the resulting ray must point up where the sky above the model
    /// is on screen and down where the ground is.
    #[test]
    fn sky_rays_from_the_inverse_point_up_above_and_down_below() {
        let mut camera = Camera::default();
        camera.fit(Bounds {
            min: [-1.; 3],
            max: [1.; 3],
        });
        let m = glam::Mat4::from_cols_array_2d(&camera.matrix(1200, 760));
        let inverse = m.inverse();
        assert!(inverse.is_finite());
        let up = camera.center + Vec3::Y * camera.radius * 0.5;
        let c = m * up.extend(1.);
        let screen_up = (c.y / c.w).signum();
        let ray = |y: f32| {
            let near = inverse.project_point3(Vec3::new(0., y, 1.));
            let far = inverse.project_point3(Vec3::new(0., y, 0.));
            (far - near).normalize()
        };
        assert!(ray(screen_up * 0.99).y > 0.);
        assert!(ray(-screen_up * 0.99).y < 0.);
    }
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
            let eye = camera.eye(1200, 760);
            camera.orbit(80., -40.);
            camera.pan(5., 4., 760.);
            camera.zoom(3.);
            let moved = camera.matrix(960, 600);
            assert_ne!(initial, moved);
            assert_ne!(eye, camera.eye(960, 600));
            assert!(camera.eye(1, 100).iter().all(|v| v.is_finite()));
            assert!(moved.iter().flatten().all(|v| v.is_finite()));
            assert!(camera.matrix(0, 0).iter().flatten().all(|v| v.is_finite()));
        }
    }
    #[test]
    fn fixed_views_look_along_their_axes_and_orbit_leaves_orthographic() {
        let mut camera = Camera::default();
        camera.fit(Bounds {
            min: [-1., 0., -1.],
            max: [1., 2., 1.],
        });
        let (w, h) = (1200, 800);
        let centre = [0., 1., 0.];
        for (view, axis) in [
            (View::Front, [0., 0., 1.]),
            (View::Back, [0., 0., -1.]),
            (View::Left, [-1., 0., 0.]),
            (View::Right, [1., 0., 0.]),
            (View::Top, [0., 1., 0.]),
            (View::Bottom, [0., -1., 0.]),
        ] {
            camera.set_view(view);
            assert!(camera.is_ortho());
            let eye = Vec3::from_array(camera.eye(w, h));
            let toward = (eye - Vec3::from_array(centre)).normalize();
            assert!(
                toward.dot(Vec3::from_array(axis)) > 0.999,
                "{view:?}: {toward}"
            );
            // The centre is in the middle of the screen and parallel lines stay
            // parallel: a step along the view axis does not move on screen.
            let c = camera.project(centre, w, h).unwrap();
            assert!(
                (c[0] - 0.5).abs() < 1e-4 && (c[1] - 0.5).abs() < 1e-4,
                "{view:?} {c:?}"
            );
            let nearer = Vec3::from_array(centre) + Vec3::from_array(axis) * 0.5;
            let n = camera.project(nearer.to_array(), w, h).unwrap();
            assert!(
                (n[0] - c[0]).abs() < 1e-4 && (n[1] - c[1]).abs() < 1e-4,
                "{view:?}"
            );
            // The model's corners are visible (inside the screen and depth range).
            let m = glam::Mat4::from_cols_array_2d(&camera.matrix(w, h));
            for corner in [[-1., 0., -1.], [1., 2., 1.], [1., 0., -1.]] {
                let clip = m * Vec3::from_array(corner).extend(1.);
                let ndc = clip.truncate() / clip.w;
                assert!(ndc.x.abs() <= 1. && ndc.y.abs() <= 1., "{view:?} {ndc}");
                assert!((0. ..=1.).contains(&ndc.z), "{view:?} depth {ndc}");
            }
        }
        // Top view: front (+Z) is at the bottom of the screen.
        camera.set_view(View::Top);
        let front = camera.project([0., 1., 1.], w, h).unwrap();
        assert!(front[1] > 0.5, "{front:?}");
        camera.orbit(10., 0.);
        assert!(!camera.is_ortho());
        camera.set_view(View::ThreeQuarter);
        assert!(!camera.is_ortho());
    }
}
