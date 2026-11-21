//! Object picking: a ray from the camera through the pointer, tested against
//! the snapshot's triangles (both sides), draw by draw with a bounding-box
//! check first. The nearest hit names a draw and so its scene node.
use crate::camera::Camera;
use ad_document::GeometrySnapshot;
use glam::{Mat4, Vec3, Vec4};

/// Axis-aligned box of every draw (index ranges of the snapshot).
pub fn draw_bounds(snapshot: &GeometrySnapshot) -> Vec<(Vec3, Vec3)> {
    snapshot
        .draws
        .iter()
        .map(|draw| {
            let start = draw.index_start as usize;
            let end = start + draw.index_count as usize;
            let mut lo = Vec3::splat(f32::INFINITY);
            let mut hi = Vec3::splat(f32::NEG_INFINITY);
            for &i in snapshot.indices.get(start..end).unwrap_or(&[]) {
                if let Some(p) = snapshot.positions.get(i as usize) {
                    let p = Vec3::from_array(*p);
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
            (lo, hi)
        })
        .collect()
}

/// World ray through viewport coordinates (0..1, y down).
pub fn ray(camera: &Camera, uv: [f32; 2], width: u32, height: u32) -> Option<(Vec3, Vec3)> {
    let inverse = Mat4::from_cols_array_2d(&camera.matrix(width, height)).inverse();
    let (x, y) = (uv[0] * 2. - 1., uv[1] * 2. - 1.);
    // Reversed-Z: near plane at depth 1, far plane at 0.
    let near = inverse * Vec4::new(x, y, 1., 1.);
    let far = inverse * Vec4::new(x, y, 0., 1.);
    let (near, far) = (near.truncate() / near.w, far.truncate() / far.w);
    let direction = (far - near).try_normalize()?;
    (near.is_finite() && direction.is_finite()).then_some((near, direction))
}

fn hits_box(origin: Vec3, inverse: Vec3, lo: Vec3, hi: Vec3, best: f32) -> bool {
    let a = (lo - origin) * inverse;
    let b = (hi - origin) * inverse;
    let enter = a.min(b).max_element();
    let leave = a.max(b).min_element();
    leave >= enter.max(0.) && enter <= best
}

/// Distance along the ray to a triangle (Möller–Trumbore, both sides).
fn triangle(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1. / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    if !(0. ..=1.).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = direction.dot(q) * inv;
    if v < 0. || u + v > 1. {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.).then_some(t)
}

/// Nearest draw hit by the ray, skipping `hidden` draws.
pub fn pick(
    snapshot: &GeometrySnapshot,
    bounds: &[(Vec3, Vec3)],
    origin: Vec3,
    direction: Vec3,
    hidden: impl Fn(usize) -> bool,
) -> Option<(usize, f32)> {
    let inverse = direction.recip();
    let mut best: Option<(usize, f32)> = None;
    for (index, draw) in snapshot.draws.iter().enumerate() {
        let limit = best.map_or(f32::INFINITY, |b| b.1);
        let Some(&(lo, hi)) = bounds.get(index) else {
            continue;
        };
        if hidden(index) || !hits_box(origin, inverse, lo, hi, limit) {
            continue;
        }
        let start = draw.index_start as usize;
        let end = start + draw.index_count as usize;
        let Some(indices) = snapshot.indices.get(start..end) else {
            continue;
        };
        for tri in indices.as_chunks::<3>().0 {
            let [Some(a), Some(b), Some(c)] = [tri[0], tri[1], tri[2]].map(|i| {
                snapshot
                    .positions
                    .get(i as usize)
                    .copied()
                    .map(Vec3::from_array)
            }) else {
                continue;
            };
            if let Some(t) = triangle(origin, direction, a, b, c)
                && best.is_none_or(|b| t < b.1)
            {
                best = Some((index, t));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use ad_document::{Cancellation, ImportLimits};
    #[test]
    fn the_ray_through_the_screen_centre_picks_the_front_object() {
        let doc = ad_formats::import_glb(
            std::sync::Arc::from(
                include_bytes!("../../../tests/fixtures/Twin_Cubes.glb").as_slice(),
            ),
            "twins".into(),
            ad_document::DocumentId(1),
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
        let snapshot = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        let bounds = draw_bounds(&snapshot);
        assert_eq!(bounds.len(), snapshot.draws.len());
        let mut camera = Camera::default();
        camera.fit(snapshot.bounds);
        // Aim at the centre of each draw's box from the camera: that draw is
        // hit (unless another one is in front), and hiding it reveals the next.
        let (w, h) = (1200, 800);
        for (index, (lo, hi)) in bounds.iter().enumerate() {
            let centre = ((*lo + *hi) * 0.5).to_array();
            let uv = camera.project(centre, w, h).unwrap();
            let (origin, direction) = ray(&camera, uv, w, h).unwrap();
            let hit = pick(&snapshot, &bounds, origin, direction, |_| false).unwrap();
            let through = pick(&snapshot, &bounds, origin, direction, |i| i == hit.0);
            assert!(
                hit.0 == index || through.is_some_and(|t| t.0 == index),
                "{index}: {hit:?}"
            );
            assert!(hit.1 > 0.);
        }
        // Empty sky is no hit.
        let (origin, direction) = ray(&camera, [0.01, 0.01], w, h).unwrap();
        assert!(pick(&snapshot, &bounds, origin, direction, |_| false).is_none());
        assert!(pick(&snapshot, &bounds, origin, direction, |_| true).is_none());
        // Orthographic views give parallel rays.
        camera.set_view(crate::camera::View::Front);
        let (_, a) = ray(&camera, [0.2, 0.5], w, h).unwrap();
        let (_, b) = ray(&camera, [0.8, 0.5], w, h).unwrap();
        assert!(a.dot(b) > 0.9999);
    }
}
