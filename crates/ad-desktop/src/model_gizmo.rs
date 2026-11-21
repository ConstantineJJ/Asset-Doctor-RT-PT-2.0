//! ADR-0041 model gizmo: axis arrows move the model, the centre square scales
//! it uniformly about the bottom centre. While dragging, the change is only a
//! renderer preview; releasing prepares one placement edit (one Undo step).
use crate::camera::Camera;
use ad_document::{Bounds, Placement};
use glam::{Mat4, Vec3};

/// Pointer travel (pixels) that doubles or halves the scale.
const SCALE_PIXELS: f32 = 140.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Axis(usize),
    Scale,
}

/// Preview of one drag: scale `factor` about `pivot`, then move by `offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Delta {
    pub pivot: [f32; 3],
    pub factor: f32,
    pub offset: [f32; 3],
}
impl Delta {
    pub fn new(pivot: [f32; 3]) -> Self {
        Self {
            pivot,
            factor: 1.,
            offset: [0.; 3],
        }
    }
    pub fn is_noop(&self) -> bool {
        self.factor == 1. && self.offset == [0.; 3]
    }
    pub fn matrix(&self) -> Mat4 {
        let pivot = Vec3::from_array(self.pivot);
        Mat4::from_translation(pivot + Vec3::from_array(self.offset))
            * Mat4::from_scale(Vec3::splat(self.factor))
            * Mat4::from_translation(-pivot)
    }
    /// The placement this drag publishes on release.
    pub fn placement(&self, current: Placement) -> Placement {
        current.then(self.pivot, self.factor, self.offset)
    }
}

/// Preview that turns the `current` placement into `target` (numeric
/// fields): scale about the world origin, then move.
pub fn between(current: Placement, target: Placement) -> Delta {
    let factor = target.scale / current.scale;
    Delta {
        pivot: [0.; 3],
        factor,
        offset: [0, 1, 2].map(|a| target.translation[a] - factor * current.translation[a]),
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
/// Drag up enlarges, down shrinks; the total scale stays inside the limits.
pub fn drag_scale(delta: &mut Delta, current: Placement, drag_y: f32) {
    let factor = delta.factor * (-drag_y / SCALE_PIXELS * std::f32::consts::LN_2).exp();
    let total = current.scale * factor;
    if total.is_finite() && (Placement::MIN_SCALE..=Placement::MAX_SCALE).contains(&total) {
        delta.factor = factor;
    }
}

/// Screen positions (viewport 0..1, y down) of the gizmo for this frame.
pub fn screen(
    camera: &Camera,
    bounds: Bounds,
    preview: Option<&Delta>,
    width: u32,
    height: u32,
) -> Option<crate::viewer_ui::ModelGizmo> {
    let mut origin = pivot(bounds);
    let mut len = handle_length(bounds);
    if let Some(p) = preview {
        origin = (Vec3::from_array(origin) + Vec3::from_array(p.offset)).to_array();
        len *= p.factor;
    }
    let project = |p: [f32; 3]| camera.project(p, width, height);
    let ends = [0, 1, 2].map(|axis| {
        let mut end = origin;
        end[axis] += len;
        project(end)
    });
    Some(crate::viewer_ui::ModelGizmo {
        pivot: project(origin)?,
        ends,
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
        let g = screen(&camera, cube(), None, w, h).unwrap();
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
}
