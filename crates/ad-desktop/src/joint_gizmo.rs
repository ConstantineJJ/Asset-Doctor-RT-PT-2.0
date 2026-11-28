//! P7 joint gizmo: rings turn the selected joint about X/Y/Z (local or world
//! axes), arrows move it along one axis and plane squares along two
//! (ADR-0063). The application builds the gizmo on screen from
//! the joint's pose; the interface turns pointer drags into a new local
//! rotation or translation, previewed while dragging and written as one key
//! of the current clip on release.
use ad_document::NodeId;
use glam::{Mat4, Quat, Vec2, Vec3};

/// Points of each ring on screen.
pub const RING_POINTS: usize = 48;
/// Default ring radius on screen, in viewport pixels (owner 2026-11-28:
/// larger, for precise fitting; «Размер» scales it 0.5–2×).
pub const RADIUS_PX: f32 = 110.;

#[derive(Clone, Debug, PartialEq)]
pub struct JointGizmo {
    pub document: u64,
    pub node: NodeId,
    pub clip: usize,
    pub time: f32,
    /// Joint centre on screen (viewport 0..1, y down).
    pub center: [f32; 2],
    /// Rings about axes 0..3 on screen; None for points behind the camera.
    pub rings: [Vec<Option<[f32; 2]>>; 3],
    /// Arrow tips on screen.
    pub arrows: [Option<[f32; 2]>; 3],
    /// Corners of the plane squares (`gizmo_paint::PLANES`) on screen.
    pub planes: [[Option<[f32; 2]>; 4]; 3],
    /// Axis directions in world space (local or world mode).
    pub axes: [[f32; 3]; 3],
    /// Viewport movement per world unit along each axis (for arrows).
    pub per_unit: [[f32; 2]; 3],
    /// +1 when the axis points away from the camera (a counter-clockwise
    /// screen turn is then a negative turn about it), else -1.
    pub away: [f32; 3],
    /// Parent world rotation and inverse parent world matrix (for turning a
    /// world change into the joint's local channels).
    pub parent_rotation: [f32; 4],
    pub parent_inverse: [[f32; 4]; 4],
    /// The joint's local rotation and translation at `time` (no preview).
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
}

/// Builds the gizmo of a joint whose world matrix is `world` and parent
/// world matrix `parent`, through `project` (world point → viewport 0..1).
#[allow(clippy::too_many_arguments)]
pub fn build(
    document: u64,
    node: NodeId,
    clip: usize,
    time: f32,
    world: Mat4,
    parent: Mat4,
    rotation: [f32; 4],
    translation: [f32; 3],
    local_axes: bool,
    radius_px: f32,
    camera_forward: Vec3,
    viewport_px: Vec2,
    project: impl Fn(Vec3) -> Option<[f32; 2]>,
) -> Option<JointGizmo> {
    let (_, joint_rotation, centre) = world.to_scale_rotation_translation();
    let center = project(centre)?;
    let (_, parent_rot, _) = parent.to_scale_rotation_translation();
    let axes: [Vec3; 3] = if local_axes {
        [Vec3::X, Vec3::Y, Vec3::Z].map(|a| (joint_rotation * a).normalize_or(a))
    } else {
        [Vec3::X, Vec3::Y, Vec3::Z]
    };
    // World size of RADIUS_PX at the joint: from one unit along the screen.
    let side = camera_forward.any_orthonormal_vector();
    let a = Vec2::from_array(project(centre)?) * viewport_px;
    let b = Vec2::from_array(project(centre + side)?) * viewport_px;
    let px_per_unit = (b - a).length();
    if !(px_per_unit.is_finite() && px_per_unit > 1e-6) {
        return None;
    }
    let radius = radius_px / px_per_unit;
    let rings = axes.map(|axis| {
        let u = axis.any_orthonormal_vector();
        let v = axis.cross(u);
        (0..RING_POINTS)
            .map(|i| {
                let t = i as f32 / RING_POINTS as f32 * std::f32::consts::TAU;
                project(centre + (u * t.cos() + v * t.sin()) * radius)
            })
            .collect()
    });
    let arrows = axes.map(|axis| project(centre + axis * radius * 1.35));
    let reach = radius * 1.35;
    let planes = crate::gizmo_paint::PLANES.map(|[u, v]| {
        let (a, b) = (
            crate::gizmo_paint::PLANE_FROM * reach,
            crate::gizmo_paint::PLANE_TO * reach,
        );
        [(a, a), (b, a), (b, b), (a, b)].map(|(s, t)| project(centre + axes[u] * s + axes[v] * t))
    });
    let per_unit = axes.map(|axis| {
        project(centre + axis).map_or([0.; 2], |p| [p[0] - center[0], p[1] - center[1]])
    });
    let away = axes.map(|axis| {
        if axis.dot(camera_forward) > 0. {
            1.
        } else {
            -1.
        }
    });
    Some(JointGizmo {
        document,
        node,
        clip,
        time,
        center,
        rings,
        arrows,
        planes,
        axes: axes.map(|a| a.to_array()),
        per_unit,
        away,
        parent_rotation: parent_rot.normalize().to_array(),
        parent_inverse: parent.inverse().to_cols_array_2d(),
        rotation,
        translation,
    })
}

impl JointGizmo {
    /// Local rotation after turning the joint by `angle` radians about world
    /// axis `axis` (0..3): L' = P⁻¹ · R · P · L in rotations.
    pub fn turned(&self, axis: usize, angle: f32) -> [f32; 4] {
        let parent = Quat::from_array(self.parent_rotation);
        let turn = Quat::from_axis_angle(Vec3::from_array(self.axes[axis]), angle);
        (parent.inverse() * turn * parent * Quat::from_array(self.rotation))
            .normalize()
            .to_array()
    }
    /// Local translation after moving the joint `distance` world units
    /// along world axis `axis`.
    pub fn moved(&self, axis: usize, distance: f32) -> [f32; 3] {
        let delta = Mat4::from_cols_array_2d(&self.parent_inverse)
            .transform_vector3(Vec3::from_array(self.axes[axis]) * distance);
        (Vec3::from_array(self.translation) + delta).to_array()
    }
    /// Local translation after moving `a` world units along the plane's
    /// first axis and `b` along its second.
    pub fn moved_in_plane(&self, plane: usize, a: f32, b: f32) -> [f32; 3] {
        let [u, v] = crate::gizmo_paint::PLANES[plane.min(2)];
        let world = Vec3::from_array(self.axes[u]) * a + Vec3::from_array(self.axes[v]) * b;
        let delta = Mat4::from_cols_array_2d(&self.parent_inverse).transform_vector3(world);
        (Vec3::from_array(self.translation) + delta).to_array()
    }
    /// World distances along the plane's two axes for a pointer move
    /// `delta` (viewport); None when the plane is seen edge-on.
    pub fn drag_plane(&self, plane: usize, delta: [f32; 2], aspect: f32) -> Option<(f32, f32)> {
        let [u, v] = crate::gizmo_paint::PLANES[plane.min(2)];
        let s = |p: [f32; 2]| [p[0] * aspect, p[1]];
        crate::gizmo_paint::split(s(delta), s(self.per_unit[u]), s(self.per_unit[v]))
    }
    /// World distance along `axis` for a pointer move `delta` (viewport).
    pub fn drag_distance(&self, axis: usize, delta: [f32; 2], aspect: f32) -> f32 {
        let scale = Vec2::new(aspect, 1.);
        let per = Vec2::from_array(self.per_unit[axis]) * scale;
        let d = Vec2::from_array(delta) * scale;
        if per.length_squared() < 1e-12 {
            return 0.;
        }
        d.dot(per) / per.length_squared()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gizmo(local: bool, parent: Mat4, world: Mat4) -> JointGizmo {
        // Orthographic view down -Z: x right, y down on screen.
        let project = |p: Vec3| Some([0.5 + p.x * 0.1, 0.5 - p.y * 0.1]);
        build(
            1,
            NodeId(3),
            0,
            0.5,
            world,
            parent,
            Quat::IDENTITY.to_array(),
            [0., 1., 0.],
            local,
            RADIUS_PX,
            Vec3::NEG_Z,
            Vec2::new(1000., 1000.),
            project,
        )
        .unwrap()
    }

    #[test]
    fn rings_and_arrows_surround_the_joint() {
        let g = gizmo(
            false,
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(0., 1., 0.)),
        );
        assert_eq!(g.center, [0.5, 0.4]);
        for ring in &g.rings {
            assert_eq!(ring.len(), RING_POINTS);
        }
        // The Z ring faces the camera: every point RADIUS_PX away.
        for p in g.rings[2].iter().flatten() {
            let d = ((p[0] - 0.5) * 1000.).hypot((p[1] - 0.4) * 1000.);
            assert!((d - RADIUS_PX).abs() < 0.5, "{d}");
        }
        let x = g.arrows[0].unwrap();
        assert!(x[0] > 0.5 && (x[1] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn turning_and_moving_give_the_local_channels() {
        // Parent turned 90 degrees about Y: a world turn about X is a local
        // turn about the parent's -Z... checked by composing back.
        let parent = Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)
            * Mat4::from_translation(Vec3::new(2., 0., 0.));
        let world = parent * Mat4::from_translation(Vec3::new(0., 1., 0.));
        let g = gizmo(false, parent, world);
        let angle = 0.6;
        let local = Quat::from_array(g.turned(0, angle));
        let (_, parent_rotation, _) = parent.to_scale_rotation_translation();
        let composed = parent_rotation * local;
        let expected = Quat::from_rotation_x(angle) * parent_rotation;
        assert!(
            composed.dot(expected).abs() > 0.99999,
            "{composed:?} {expected:?}"
        );
        // Moving 1.5 along world X moves the joint 1.5 along world X.
        let t = Vec3::from_array(g.moved(0, 1.5));
        let moved = parent * Mat4::from_translation(t);
        let delta = moved.w_axis.truncate() - world.w_axis.truncate();
        assert!(delta.abs_diff_eq(Vec3::new(1.5, 0., 0.), 1e-5), "{delta}");
        // Local axes follow the joint's own turn.
        let turned = gizmo(
            true,
            Mat4::IDENTITY,
            Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
        );
        assert!(Vec3::from_array(turned.axes[0]).abs_diff_eq(Vec3::Y, 1e-5));
    }

    #[test]
    fn screen_drags_become_angles_and_distances() {
        let g = gizmo(false, Mat4::IDENTITY, Mat4::IDENTITY);
        // The camera looks down -Z, so +Z points at it: a screen turn about
        // it has the opposite sign of one about an axis pointing away.
        assert_eq!(g.away[2], -1.);
        // Dragging 0.2 to the right along X is 2 world units here.
        let d = g.drag_distance(0, [0.2, 0.], 1.);
        assert!((d - 2.).abs() < 1e-4, "{d}");
        // Up on screen along Y (y down) is +Y.
        assert!(g.drag_distance(1, [0., -0.1], 1.) > 0.);
        // The XY square: right and up move along +X and +Y at once.
        let (a, b) = g.drag_plane(0, [0.2, -0.1], 1.).unwrap();
        assert!((a - 2.).abs() < 1e-4 && (b - 1.).abs() < 1e-4, "{a} {b}");
        let t = g.moved_in_plane(0, a, b);
        assert!(
            Vec3::from_array(t).abs_diff_eq(Vec3::new(2., 2., 0.), 1e-4),
            "{t:?}"
        );
        // Seen along Z, the YZ and XZ squares are edge-on.
        assert!(g.drag_plane(1, [0.1, 0.1], 1.).is_none());
        assert!(g.planes[0].iter().all(|c| c.is_some()));
    }
}
