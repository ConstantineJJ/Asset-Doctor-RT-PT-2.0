//! ADR-0041 model gizmo: axis arrows move the model, plane squares move it in
//! two axes (along the floor, along a wall), the centre square scales it
//! uniformly and three rings turn it about X, Y and Z, all about the bottom
//! centre. A ring turns by the pointer's angle
//! around the centre (ADR-0063). While dragging, the change is only
//! a renderer preview; releasing prepares one placement edit (one Undo step).
use crate::camera::Camera;
use ad_document::{Bounds, Placement};
use glam::{Mat4, Quat, Vec3};

/// Pointer travel (pixels) that doubles or halves the scale.
const SCALE_PIXELS: f32 = 140.;
/// Points of one rotation ring on screen.
pub const RING_POINTS: usize = 48;
/// The rings the gizmo offers: X, Y and Z (owner 2026-11-28: all three).
pub const RING_AXES: [usize; 3] = [0, 1, 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Axis(usize),
    /// Move in two axes: 0 = XY, 1 = YZ, 2 = XZ (the floor).
    Plane(usize),
    Scale,
    /// Turn about world axis 0 (X), 1 (Y) or 2 (Z).
    Rotate(usize),
}

/// Preview of one drag: `x' = factor * rotation(x - pivot) + pivot + offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delta {
    pub pivot: [f32; 3],
    pub factor: f32,
    pub rotation: [f32; 4],
    pub offset: [f32; 3],
    /// Radians turned by a ring drag (moves the ring's knob).
    pub turned: f32,
}
impl Delta {
    pub fn new(pivot: [f32; 3]) -> Self {
        Self {
            pivot,
            factor: 1.,
            rotation: Placement::IDENTITY.rotation,
            offset: [0.; 3],
            turned: 0.,
        }
    }
    pub fn is_noop(&self) -> bool {
        self.factor == 1. && self.offset == [0.; 3] && self.rotation == Placement::IDENTITY.rotation
    }
    pub fn matrix(&self) -> Mat4 {
        let pivot = Vec3::from_array(self.pivot);
        Mat4::from_translation(pivot + Vec3::from_array(self.offset))
            * Mat4::from_quat(Quat::from_array(self.rotation))
            * Mat4::from_scale(Vec3::splat(self.factor))
            * Mat4::from_translation(-pivot)
    }
    /// The placement this drag publishes on release.
    pub fn placement(&self, current: Placement) -> Placement {
        current.transformed(self.pivot, self.factor, self.rotation, self.offset)
    }
}

/// Preview that turns the `current` placement into `target` (numeric
/// fields): scale and turn about the world origin, then move.
pub fn between(current: Placement, target: Placement) -> Delta {
    let factor = target.scale / current.scale;
    let turn = if target.rotation == current.rotation {
        Quat::IDENTITY
    } else {
        (target.quat() * current.quat().inverse()).normalize()
    };
    let moved = turn * Vec3::from_array(current.translation) * factor;
    Delta {
        pivot: [0.; 3],
        factor,
        rotation: turn.to_array(),
        offset: [0, 1, 2].map(|a| target.translation[a] - moved[a]),
        turned: 0.,
    }
}
/// «На пол»: moves the model so its lowest point is at Y = 0.
pub fn on_floor(current: Placement, bounds: Bounds) -> Placement {
    let mut p = current;
    p.translation[1] -= bounds.min[1];
    p
}
/// «В центр»: moves the model so its bottom centre is above the origin.
pub fn centred(current: Placement, bounds: Bounds) -> Placement {
    let mut p = current;
    let c = pivot(bounds);
    p.translation[0] -= c[0];
    p.translation[2] -= c[2];
    p
}

/// Numeric fields (ADR-0041): what the owner edits, measured at the
/// imported model's bottom centre `p0`, so turning and scaling (both about
/// that point) never change «Сдвиг».
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fields {
    /// Where the imported bottom centre moved, relative to where it was.
    pub offset: [f32; 3],
    /// Turn about Y and X in degrees (Euler Y-X-Z; Z is kept as it is).
    pub yaw: f32,
    pub pitch: f32,
    pub scale: f32,
}
impl Fields {
    pub fn of(placement: Placement, source: Bounds) -> Self {
        let p0 = pivot(source);
        let placed = placement.apply(p0);
        let (yaw, pitch, _) = placement.quat().to_euler(glam::EulerRot::YXZ);
        Self {
            offset: [0, 1, 2].map(|a| placed[a] - p0[a]),
            yaw: yaw.to_degrees(),
            pitch: pitch.to_degrees(),
            scale: placement.scale,
        }
    }
    /// The placement that shows these fields, starting from `current`.
    pub fn placement(self, current: Placement, source: Bounds) -> Placement {
        let was = Self::of(current, source);
        let p0 = pivot(source);
        let mut p = current;
        if self.yaw != was.yaw || self.pitch != was.pitch {
            let (_, _, roll) = current.quat().to_euler(glam::EulerRot::YXZ);
            let target = Quat::from_euler(
                glam::EulerRot::YXZ,
                self.yaw.to_radians(),
                self.pitch.to_radians(),
                roll,
            );
            let turn = (target * current.quat().inverse()).normalize();
            p = p.transformed(p.apply(p0), 1., turn.to_array(), [0.; 3]);
        }
        if self.scale != was.scale && self.scale > 0. {
            p = p.then(p.apply(p0), self.scale / p.scale, [0.; 3]);
            p.scale = self.scale;
        }
        let placed = p.apply(p0);
        for a in 0..3 {
            p.translation[a] += p0[a] + self.offset[a] - placed[a];
        }
        p
    }
}

/// Bottom centre of the bounds: X/Z centre, lowest Y.
pub fn pivot(bounds: Bounds) -> [f32; 3] {
    [
        (bounds.min[0] + bounds.max[0]) * 0.5,
        bounds.min[1],
        (bounds.min[2] + bounds.max[2]) * 0.5,
    ]
}
/// Arrow length in world units: a share of the largest extent.
pub fn handle_length(bounds: Bounds) -> f32 {
    let extent = (0..3)
        .map(|a| bounds.max[a] - bounds.min[a])
        .fold(0f32, f32::max);
    (extent * 0.45).max(1e-6)
}
/// Rotation ring radius in world units.
pub fn ring_radius(bounds: Bounds) -> f32 {
    handle_length(bounds) * 1.25
}
fn axis_vector(axis: usize) -> Vec3 {
    [Vec3::X, Vec3::Y, Vec3::Z][axis.min(2)]
}
/// Unit vector of a ring at parameter `angle`: the X ring lies in the YZ
/// plane, the Y ring in the XZ plane, the Z ring in the XY plane.
fn ring_vector(axis: usize, angle: f32) -> Vec3 {
    let (s, c) = angle.sin_cos();
    match axis {
        0 => Vec3::new(0., c, s),
        1 => Vec3::new(c, 0., s),
        _ => Vec3::new(c, s, 0.),
    }
}
/// Ring parameter of the point nearest to the eye: where the knob starts.
pub fn knob_angle(axis: usize, center: [f32; 3], eye: [f32; 3]) -> f32 {
    let d = Vec3::from_array(eye) - Vec3::from_array(center);
    match axis {
        0 => d.z.atan2(d.y),
        1 => d.z.atan2(d.x),
        _ => d.y.atan2(d.x),
    }
}
/// World position of a ring knob after turning `turned` radians.
pub fn knob(axis: usize, center: [f32; 3], radius: f32, base: f32, turned: f32) -> [f32; 3] {
    (Vec3::from_array(center)
        + Quat::from_axis_angle(axis_vector(axis), turned) * ring_vector(axis, base) * radius)
        .to_array()
}

/// Moves along `axis` so the arrow tip follows the pointer: the drag (pixels)
/// is projected on the arrow's screen direction `axis_px` (pixels for `len`).
pub fn drag_axis(delta: &mut Delta, axis: usize, len: f32, drag: [f32; 2], axis_px: [f32; 2]) {
    let length2 = axis_px[0] * axis_px[0] + axis_px[1] * axis_px[1];
    if length2 < 4. {
        return; // Arrow points at the camera: no reliable direction.
    }
    let along = (drag[0] * axis_px[0] + drag[1] * axis_px[1]) / length2;
    delta.offset[axis] += along * len;
}
/// Moves in the plane of axes `[u, v]` so the square follows the pointer:
/// `u_px` and `v_px` are the screen motions (pixels) for `len` along each.
/// A plane seen edge-on does not move.
pub fn drag_plane(
    delta: &mut Delta,
    plane: usize,
    len: f32,
    drag: [f32; 2],
    u_px: [f32; 2],
    v_px: [f32; 2],
) {
    let [u, v] = crate::gizmo_paint::PLANES[plane.min(2)];
    if let Some((a, b)) = crate::gizmo_paint::split(drag, u_px, v_px) {
        delta.offset[u] += a * len;
        delta.offset[v] += b * len;
    }
}
/// Drag up enlarges, down shrinks; the total scale stays inside the limits.
pub fn drag_scale(delta: &mut Delta, current: Placement, drag_y: f32) {
    let factor = delta.factor * (-drag_y / SCALE_PIXELS * std::f32::consts::LN_2).exp();
    let total = current.scale * factor;
    if total.is_finite() && (Placement::MIN_SCALE..=Placement::MAX_SCALE).contains(&total) {
        delta.factor = factor;
    }
}
/// Turns about `axis` by `step`, the pointer's screen angle step around
/// the ring centre (radians, y down, from `gizmo_paint::TurnTrack`) — the
/// ring can be grabbed anywhere. `facing` is the dot product of the axis
/// with the direction towards the eye.
pub fn drag_ring(delta: &mut Delta, axis: usize, step: f32, facing: f32) {
    if !step.is_finite() {
        return;
    }
    // Clockwise on a y-down screen is a negative turn about an axis that
    // points at the eye.
    let sign = if facing >= 0. { -1. } else { 1. };
    delta.turned += sign * step.clamp(-0.5, 0.5);
    delta.rotation = Quat::from_axis_angle(axis_vector(axis), delta.turned)
        .normalize()
        .to_array();
}

/// Screen positions (viewport 0..1, y down) of the gizmo for this frame.
/// `turning`: the ring being dragged, whose knob follows the preview.
pub fn screen(
    camera: &Camera,
    bounds: Bounds,
    preview: Option<&Delta>,
    turning: Option<usize>,
    width: u32,
    height: u32,
) -> Option<crate::viewer_ui::ModelGizmo> {
    let base = pivot(bounds);
    let identity = Delta::new(base);
    let d = preview.unwrap_or(&identity);
    let origin = d
        .matrix()
        .transform_point3(Vec3::from_array(base))
        .to_array();
    let len = handle_length(bounds) * d.factor;
    let radius = ring_radius(bounds) * d.factor;
    let project = |p: [f32; 3]| camera.project(p, width, height);
    let ends = [0, 1, 2].map(|axis| {
        let mut end = origin;
        end[axis] += len;
        project(end)
    });
    let planes = crate::gizmo_paint::PLANES.map(|[u, v]| {
        let (a, b) = (
            crate::gizmo_paint::PLANE_FROM * len,
            crate::gizmo_paint::PLANE_TO * len,
        );
        [(a, a), (b, a), (b, b), (a, b)].map(|(s, t)| {
            let mut p = origin;
            p[u] += s;
            p[v] += t;
            project(p)
        })
    });
    let eye = camera.eye(width, height);
    let mut rings = [[None; RING_POINTS]; 3];
    let mut knobs = [None; 3];
    for (slot, axis) in RING_AXES.into_iter().enumerate() {
        for (i, point) in rings[slot].iter_mut().enumerate() {
            let a = i as f32 / RING_POINTS as f32 * std::f32::consts::TAU;
            *point = project(knob(axis, origin, radius, a, 0.));
        }
        let turned = if turning == Some(axis) { d.turned } else { 0. };
        knobs[slot] = project(knob(
            axis,
            origin,
            radius,
            knob_angle(axis, origin, eye),
            turned,
        ));
    }
    Some(crate::viewer_ui::ModelGizmo {
        pivot: project(origin)?,
        ends,
        planes,
        rings,
        knobs,
        turned: turning.map(|_| d.turned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cube() -> Bounds {
        Bounds {
            min: [-1., 0., -1.],
            max: [1., 2., 1.],
        }
    }
    #[test]
    fn delta_matches_the_published_placement_and_keeps_the_pivot_when_scaling() {
        let mut d = Delta::new(pivot(cube()));
        assert!(d.is_noop());
        d.factor = 2.;
        d.offset = [0.5, 0., 0.];
        let current = Placement::IDENTITY.then([0.; 3], 1.5, [0., 1., 0.]);
        let placed = d.placement(current);
        for p in [[0., 0., 0.], [1., 2., 3.], [-4., 0.5, 2.]] {
            let via_delta = d
                .matrix()
                .transform_point3(Vec3::from_array(current.apply(p)));
            let direct = placed.apply(p);
            assert!(
                via_delta
                    .to_array()
                    .iter()
                    .zip(direct)
                    .all(|(a, b)| (a - b).abs() < 1e-4)
            );
        }
        let only_scale = Delta {
            factor: 3.,
            ..Delta::new(pivot(cube()))
        };
        let fixed = only_scale
            .matrix()
            .transform_point3(Vec3::from_array(pivot(cube())));
        assert_eq!(fixed.to_array(), pivot(cube()), "bottom centre stays put");
    }
    #[test]
    fn numeric_targets_preview_exactly_and_floor_and_centre_move_the_model() {
        let current = Placement::IDENTITY.then([0.; 3], 1.5, [2., -1., 0.5]);
        let target = Placement {
            scale: 3.,
            translation: [-4., 2., 7.],
            ..Placement::IDENTITY
        };
        let d = between(current, target);
        let got = d.placement(current);
        assert!((got.scale - target.scale).abs() < 1e-6);
        for a in 0..3 {
            assert!((got.translation[a] - target.translation[a]).abs() < 1e-5);
        }
        assert!(between(current, current).is_noop());
        // Current bounds are already placed (post-placement).
        let bounds = Bounds {
            min: [1., -2., 3.],
            max: [5., 4., 7.],
        };
        let grounded = on_floor(current, bounds);
        let low = grounded.apply(current.unapply(bounds.min));
        assert!(low[1].abs() < 1e-5, "{low:?}");
        let centred = centred(current, bounds);
        let c = centred.apply(current.unapply(pivot(bounds)));
        assert!(
            c[0].abs() < 1e-5 && c[2].abs() < 1e-5 && (c[1] + 2.).abs() < 1e-5,
            "{c:?}"
        );
        assert_eq!(centred.scale, current.scale);
    }
    #[test]
    fn dragging_an_arrow_moves_along_its_axis_with_the_pointer() {
        let mut camera = Camera::default();
        camera.fit(cube());
        let (w, h) = (1200, 760);
        let mut d = Delta::new(pivot(cube()));
        let len = handle_length(cube());
        let g = screen(&camera, cube(), None, None, w, h).unwrap();
        for axis in 0..3 {
            let end = g.ends[axis].unwrap();
            let axis_px = [
                (end[0] - g.pivot[0]) * w as f32,
                (end[1] - g.pivot[1]) * h as f32,
            ];
            let before = d.offset[axis];
            // Drag exactly to the arrow tip: one arrow length along the axis.
            drag_axis(&mut d, axis, len, axis_px, axis_px);
            assert!((d.offset[axis] - before - len).abs() < 1e-4);
            // Perpendicular drag does not move it.
            drag_axis(&mut d, axis, len, [-axis_px[1], axis_px[0]], axis_px);
            assert!((d.offset[axis] - before - len).abs() < 1e-4);
        }
        // A plane square moves in both of its axes with the pointer.
        let (ex, ez) = (g.ends[0].unwrap(), g.ends[2].unwrap());
        let px = |e: [f32; 2]| {
            [
                (e[0] - g.pivot[0]) * w as f32,
                (e[1] - g.pivot[1]) * h as f32,
            ]
        };
        let (ux, uz) = (px(ex), px(ez));
        let mut p = Delta::new(pivot(cube()));
        let drag = [ux[0] * 0.5 + uz[0] * 0.25, ux[1] * 0.5 + uz[1] * 0.25];
        drag_plane(&mut p, 2, len, drag, ux, uz);
        assert!((p.offset[0] - 0.5 * len).abs() < 1e-3, "{:?}", p.offset);
        assert!((p.offset[2] - 0.25 * len).abs() < 1e-3, "{:?}", p.offset);
        assert_eq!(p.offset[1], 0., "the floor square keeps the height");
        for corner in &g.planes[2] {
            assert!(corner.is_some());
        }
        let mut s = Delta::new(pivot(cube()));
        drag_scale(&mut s, Placement::IDENTITY, -SCALE_PIXELS);
        assert!((s.factor - 2.).abs() < 1e-4, "{}", s.factor);
        drag_scale(&mut s, Placement::IDENTITY, 2. * SCALE_PIXELS);
        assert!((s.factor - 0.5).abs() < 1e-4);
        let tiny = Placement {
            scale: Placement::MIN_SCALE,
            ..Placement::IDENTITY
        };
        let mut t = Delta::new([0.; 3]);
        drag_scale(&mut t, tiny, 50.);
        assert_eq!(t.factor, 1., "never below the minimum scale");
    }
    fn near(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < eps)
    }
    #[test]
    fn turning_a_ring_matches_the_published_placement_and_keeps_the_pivot() {
        let mut camera = Camera::default();
        camera.fit(cube());
        let (w, h) = (1200, 760);
        let current = Placement::IDENTITY.then([0.; 3], 1.5, [0.5, 0., 0.]);
        for axis in RING_AXES {
            let mut d = Delta::new(pivot(cube()));
            d.turned = 0.5;
            d.rotation = Quat::from_axis_angle(axis_vector(axis), 0.5).to_array();
            assert!(!d.is_noop());
            let placed = d.placement(current);
            assert!(placed.valid());
            for p in [[0., 0., 0.], [1., 2., 3.], [-4., 0.5, 2.]] {
                let via = d
                    .matrix()
                    .transform_point3(Vec3::from_array(current.apply(p)));
                assert!(near(via.to_array(), placed.apply(p), 1e-4));
            }
            let fixed = d.matrix().transform_point3(Vec3::from_array(pivot(cube())));
            assert!(near(fixed.to_array(), pivot(cube()), 1e-5));
            let g = screen(&camera, cube(), Some(&d), Some(axis), w, h).unwrap();
            let slot = RING_AXES.iter().position(|a| *a == axis).unwrap();
            assert!(g.knobs[slot].is_some());
            assert_eq!(g.turned, Some(0.5), "the angle shown while turning");
            assert!(g.rings[slot].iter().filter(|p| p.is_some()).count() > RING_POINTS / 2);
        }
    }
    #[test]
    fn grabbing_a_ring_anywhere_turns_with_the_pointer_around_its_centre() {
        let mut camera = Camera::default();
        camera.fit(cube());
        let (w, h) = (1200, 800);
        let base = pivot(cube());
        let radius = ring_radius(cube());
        let centre = camera.project(base, w, h).unwrap();
        let px = |p: [f32; 3]| {
            let s = camera.project(p, w, h).unwrap();
            [s[0] * w as f32, s[1] * h as f32]
        };
        let c = [centre[0] * w as f32, centre[1] * h as f32];
        let eye = Vec3::from_array(camera.eye(w, h)) - Vec3::from_array(base);
        // Y ring: move the pointer from one ring point to the next one along
        // a positive turn; the delta turns the same way.
        let axis = 1;
        let facing = Vec3::Y.dot(eye.normalize());
        let from = knob(axis, base, radius, 0.4, 0.);
        let to = knob(axis, base, radius, 0.4, 0.2);
        let pos = |p: [f32; 2]| egui::pos2(p[0], p[1]);
        let centre = pos(c);
        let mut track = crate::gizmo_paint::TurnTrack::new(centre, pos(px(from))).unwrap();
        let mut d = Delta::new(base);
        let step = track.step(centre, pos(px(to)));
        drag_ring(&mut d, axis, step, facing);
        assert!((d.turned - 0.2).abs() < 0.05, "{}", d.turned);
        // Back again: no turn left.
        let step = track.step(centre, pos(px(from)));
        drag_ring(&mut d, axis, step, facing);
        assert!(d.turned.abs() < 1e-4, "{}", d.turned);
        // The Z ring (owner 2026-11-28) turns the same way along itself; seen
        // at a slant, its screen angle is not the world angle (as for every
        // ring grabbed anywhere), so only the direction and size are checked.
        let z_from = knob(2, base, radius, 0.4, 0.);
        let z_to = knob(2, base, radius, 0.4, 0.2);
        let mut z_track = crate::gizmo_paint::TurnTrack::new(centre, pos(px(z_from))).unwrap();
        let mut z = Delta::new(base);
        let step = z_track.step(centre, pos(px(z_to)));
        drag_ring(&mut z, 2, step, Vec3::Z.dot(eye.normalize()));
        assert!((0.1..0.4).contains(&z.turned), "{}", z.turned);
        let turned = Quat::from_array(z.rotation).to_axis_angle();
        assert!(turned.0.z.abs() > 0.999, "about Z: {turned:?}");
        // The wrap at ±π does not jump.
        let mut t =
            crate::gizmo_paint::TurnTrack::new(centre, pos([c[0] - 100., c[1] + 1.])).unwrap();
        let mut e = Delta::new(base);
        let step = t.step(centre, pos([c[0] - 100., c[1] - 1.]));
        drag_ring(&mut e, axis, step, 1.);
        assert!(e.turned.abs() < 0.05, "{}", e.turned);
    }
    #[test]
    fn numeric_fields_round_trip_and_turning_keeps_the_offset() {
        let source = cube();
        let current = Placement::IDENTITY.then([0.; 3], 2., [1., 0.5, -2.]);
        let f = Fields::of(current, source);
        assert!((f.scale - 2.).abs() < 1e-6);
        let back = f.placement(current, source);
        assert!(near(back.translation, current.translation, 1e-5));
        let mut turned = f;
        turned.yaw = 30.;
        turned.pitch = -20.;
        let p = turned.placement(current, source);
        let g = Fields::of(p, source);
        assert!(
            (g.yaw - 30.).abs() < 1e-3 && (g.pitch + 20.).abs() < 1e-3,
            "{g:?}"
        );
        assert!(near(g.offset, f.offset, 1e-4), "{g:?} vs {f:?}");
        assert!((g.scale - 2.).abs() < 1e-6);
        let mut moved = g;
        moved.offset = [3., 0., 1.];
        moved.scale = 0.5;
        let q = moved.placement(p, source);
        let h = Fields::of(q, source);
        assert!(near(h.offset, [3., 0., 1.], 1e-4), "{h:?}");
        assert!((h.scale - 0.5).abs() < 1e-6 && (h.yaw - 30.).abs() < 1e-3);
        // Previewing numeric targets with rotation is exact.
        let d = between(current, q);
        let got = d.placement(current);
        assert!((got.scale - q.scale).abs() < 1e-6);
        assert!(near(got.translation, q.translation, 1e-4));
        assert!(got.quat().angle_between(q.quat()) < 1e-4);
    }
}
