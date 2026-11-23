//! Object picking: a ray from the camera through the pointer, tested against
//! the snapshot's triangles (both sides), draw by draw with a bounding-box
//! check first. Large draws get a bounding volume tree, built on a
//! background thread; until it is ready they are tested triangle by triangle.
//! A MASK material is hit only where the renderer keeps the fragment
//! (alpha = factor × vertex colour × base colour texture ≥ cutoff), so a click
//! through a transparent part reaches what is behind it. The nearest hit
//! names a draw and so its scene node.
use crate::camera::Camera;
use ad_document::{DrawRange, GeometrySnapshot, Texture, Wrap};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

/// Draws with at least this many triangles are searched through a tree.
const TREE_MIN_TRIANGLES: usize = 2048;
/// Triangles per tree leaf.
const LEAF_TRIANGLES: usize = 8;

/// Picking data of one snapshot: the box of every draw and, for large draws,
/// a tree (filled in by a background thread; dropping the data stops it).
pub struct PickData {
    pub bounds: Vec<(Vec3, Vec3)>,
    trees: Arc<[OnceLock<Option<Tree>>]>,
    stop: Arc<AtomicBool>,
}

impl Drop for PickData {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl PickData {
    fn empty(snapshot: &GeometrySnapshot) -> Self {
        Self {
            bounds: draw_bounds(snapshot),
            trees: snapshot.draws.iter().map(|_| OnceLock::new()).collect(),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
    /// Draws worth a tree.
    fn large(snapshot: &GeometrySnapshot, tree_min: usize) -> Vec<usize> {
        (0..snapshot.draws.len())
            .filter(|&i| draw_indices(snapshot, &snapshot.draws[i]).len() / 3 >= tree_min)
            .collect()
    }
    /// Boxes now, trees of large draws in the background.
    pub fn new(snapshot: &Arc<GeometrySnapshot>) -> Self {
        let data = Self::empty(snapshot);
        let large = Self::large(snapshot, TREE_MIN_TRIANGLES);
        if !large.is_empty() {
            let (snapshot, trees, stop) = (
                Arc::clone(snapshot),
                Arc::clone(&data.trees),
                Arc::clone(&data.stop),
            );
            let spawned = std::thread::Builder::new()
                .name("pick-tree".into())
                .spawn(move || {
                    for i in large {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = trees[i].set(Tree::build(&snapshot, &snapshot.draws[i], &stop));
                    }
                });
            if let Err(error) = spawned {
                // Picking still works, triangle by triangle.
                eprintln!("pick tree thread not started: {error}");
            }
        }
        data
    }
    /// Trees for draws of at least `tree_min` triangles, built right away.
    #[cfg(test)]
    fn built(snapshot: &GeometrySnapshot, tree_min: usize) -> Self {
        let data = Self::empty(snapshot);
        for i in Self::large(snapshot, tree_min) {
            let _ = data.trees[i].set(Tree::build(snapshot, &snapshot.draws[i], &data.stop));
        }
        data
    }
    fn tree(&self, draw: usize) -> Option<&Tree> {
        self.trees.get(draw)?.get()?.as_ref()
    }
}

/// Axis-aligned box of every draw (index ranges of the snapshot).
pub fn draw_bounds(snapshot: &GeometrySnapshot) -> Vec<(Vec3, Vec3)> {
    snapshot
        .draws
        .iter()
        .map(|draw| {
            let mut lo = Vec3::splat(f32::INFINITY);
            let mut hi = Vec3::splat(f32::NEG_INFINITY);
            for &i in draw_indices(snapshot, draw) {
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

fn draw_indices<'a>(snapshot: &'a GeometrySnapshot, draw: &DrawRange) -> &'a [u32] {
    let start = draw.index_start as usize;
    let end = start + draw.index_count as usize;
    snapshot.indices.get(start..end).unwrap_or(&[])
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

/// Distance along the ray to a triangle and the weights of `b` and `c`
/// (Möller–Trumbore, both sides).
fn triangle(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, f32, f32)> {
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
    (t > 0.).then_some((t, u, v))
}

/// One wrapped texel coordinate (glTF sampler wrap modes).
fn wrap(i: i64, size: u32, mode: Wrap) -> usize {
    let n = i64::from(size.max(1));
    (match mode {
        Wrap::ClampToEdge => i.clamp(0, n - 1),
        Wrap::Repeat => i.rem_euclid(n),
        Wrap::MirroredRepeat => {
            let k = i.rem_euclid(2 * n);
            if k < n { k } else { 2 * n - 1 - k }
        }
    }) as usize
}

/// Bilinear alpha (0..1) of the base level at `uv`.
fn texture_alpha(texture: &Texture, uv: Vec2) -> f32 {
    let image = &texture.image;
    let (w, h) = (image.width, image.height);
    if w == 0 || h == 0 || !uv.is_finite() {
        return 1.;
    }
    let x = uv.x * w as f32 - 0.5;
    let y = uv.y * h as f32 - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let alpha = |dx: i64, dy: i64| {
        let i = wrap(x0 as i64 + dx, w, texture.sampler.u);
        let j = wrap(y0 as i64 + dy, h, texture.sampler.v);
        image
            .rgba
            .get((j * w as usize + i) * 4 + 3)
            .map_or(1., |a| f32::from(*a) / 255.)
    };
    let top = alpha(0, 0) * (1. - fx) + alpha(1, 0) * fx;
    let bottom = alpha(0, 1) * (1. - fx) + alpha(1, 1) * fx;
    top * (1. - fy) + bottom * fy
}

/// Whether the renderer keeps the fragment at weights (u, v) of the
/// triangle: always for OPAQUE, alpha ≥ cutoff for MASK.
fn kept(snapshot: &GeometrySnapshot, draw: &DrawRange, tri: [u32; 3], u: f32, v: f32) -> bool {
    let Some(cutoff) = draw.alpha_cutoff else {
        return true;
    };
    let w = [1. - u - v, u, v];
    let mut alpha = draw.base_color[3];
    if !snapshot.colors.is_empty() {
        alpha *= tri
            .iter()
            .zip(w)
            .map(|(&i, k)| snapshot.colors.get(i as usize).map_or(1., |c| c[3]) * k)
            .sum::<f32>();
    }
    if let Some(texture) = &draw.color_texture {
        let uv = tri.iter().zip(w).fold(Vec2::ZERO, |sum, (&i, k)| {
            sum + Vec2::from_array(snapshot.uvs.get(i as usize).copied().unwrap_or([0.; 2])) * k
        });
        alpha *= texture_alpha(texture, uv);
    }
    alpha >= cutoff
}

/// Ray test of one triangle of a draw (`first` = offset of its indices).
fn hit_triangle(
    snapshot: &GeometrySnapshot,
    draw: &DrawRange,
    tri: [u32; 3],
    origin: Vec3,
    direction: Vec3,
) -> Option<f32> {
    let [Some(a), Some(b), Some(c)] = tri.map(|i| {
        snapshot
            .positions
            .get(i as usize)
            .copied()
            .map(Vec3::from_array)
    }) else {
        return None;
    };
    let (t, u, v) = triangle(origin, direction, a, b, c)?;
    kept(snapshot, draw, tri, u, v).then_some(t)
}

/// Bounding volume tree over the triangles of one draw.
struct Tree {
    nodes: Vec<TreeNode>,
    /// Triangle numbers (within the draw), grouped by leaf.
    order: Vec<u32>,
}
#[derive(Clone, Copy)]
struct TreeNode {
    lo: Vec3,
    hi: Vec3,
    /// Leaf: first entry of `order`; inner node: index of the left child
    /// (the right one follows it).
    first: u32,
    /// Triangles of a leaf; 0 for an inner node.
    count: u32,
}

impl Tree {
    /// Median split along the longest axis of the triangle centres.
    /// None when there is nothing to search or `stop` was raised.
    fn build(snapshot: &GeometrySnapshot, draw: &DrawRange, stop: &AtomicBool) -> Option<Self> {
        let tris = draw_indices(snapshot, draw).as_chunks::<3>().0;
        let mut boxes = Vec::with_capacity(tris.len());
        let mut order = Vec::with_capacity(tris.len());
        for (n, tri) in tris.iter().enumerate() {
            if n % 65536 == 0 && stop.load(Ordering::Relaxed) {
                return None;
            }
            let [Some(a), Some(b), Some(c)] = tri.map(|i| {
                snapshot
                    .positions
                    .get(i as usize)
                    .copied()
                    .map(Vec3::from_array)
            }) else {
                boxes.push((Vec3::ZERO, Vec3::ZERO));
                continue;
            };
            boxes.push((a.min(b).min(c), a.max(b).max(c)));
            order.push(u32::try_from(n).ok()?);
        }
        if order.is_empty() {
            return None;
        }
        let centre = |t: u32| {
            let (lo, hi) = boxes[t as usize];
            (lo + hi) * 0.5
        };
        let empty = TreeNode {
            lo: Vec3::ZERO,
            hi: Vec3::ZERO,
            first: 0,
            count: 0,
        };
        let mut nodes = vec![empty];
        let mut stack = vec![(0usize, 0usize, order.len())];
        while let Some((node, start, end)) = stack.pop() {
            if stop.load(Ordering::Relaxed) {
                return None;
            }
            let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            let (mut clo, mut chi) = (lo, hi);
            for &t in &order[start..end] {
                let (a, b) = boxes[t as usize];
                (lo, hi) = (lo.min(a), hi.max(b));
                let c = centre(t);
                (clo, chi) = (clo.min(c), chi.max(c));
            }
            nodes[node].lo = lo;
            nodes[node].hi = hi;
            let extent = chi - clo;
            if end - start <= LEAF_TRIANGLES || extent.max_element() <= 0. {
                nodes[node].first = start as u32;
                nodes[node].count = (end - start) as u32;
                continue;
            }
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let middle = (start + end) / 2;
            order[start..end].select_nth_unstable_by(middle - start, |a, b| {
                centre(*a)[axis].total_cmp(&centre(*b)[axis])
            });
            let left = nodes.len();
            nodes.push(empty);
            nodes.push(empty);
            nodes[node].first = left as u32;
            stack.push((left, start, middle));
            stack.push((left + 1, middle, end));
        }
        Some(Self { nodes, order })
    }

    fn trace(
        &self,
        snapshot: &GeometrySnapshot,
        draw: &DrawRange,
        origin: Vec3,
        direction: Vec3,
        mut best: f32,
    ) -> Option<f32> {
        let inverse = direction.recip();
        let tris = draw_indices(snapshot, draw).as_chunks::<3>().0;
        let mut found = None;
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = self.nodes[index];
            if !hits_box(origin, inverse, node.lo, node.hi, best) {
                continue;
            }
            if node.count == 0 {
                stack.push(node.first as usize);
                stack.push(node.first as usize + 1);
                continue;
            }
            let first = node.first as usize;
            for &t in &self.order[first..first + node.count as usize] {
                if let Some(t) = tris
                    .get(t as usize)
                    .and_then(|tri| hit_triangle(snapshot, draw, *tri, origin, direction))
                    && t < best
                {
                    best = t;
                    found = Some(t);
                }
            }
        }
        found
    }
}

/// Nearest draw hit by the ray, skipping `hidden` draws.
pub fn pick(
    snapshot: &GeometrySnapshot,
    data: &PickData,
    origin: Vec3,
    direction: Vec3,
    hidden: impl Fn(usize) -> bool,
) -> Option<(usize, f32)> {
    let inverse = direction.recip();
    let mut best: Option<(usize, f32)> = None;
    for (index, draw) in snapshot.draws.iter().enumerate() {
        let limit = best.map_or(f32::INFINITY, |b| b.1);
        let Some(&(lo, hi)) = data.bounds.get(index) else {
            continue;
        };
        if hidden(index) || !hits_box(origin, inverse, lo, hi, limit) {
            continue;
        }
        let indices = draw_indices(snapshot, draw);
        if let Some(tree) = data.tree(index) {
            if let Some(t) = tree.trace(snapshot, draw, origin, direction, limit) {
                best = Some((index, t));
            }
            continue;
        }
        for tri in indices.as_chunks::<3>().0 {
            if let Some(t) = hit_triangle(snapshot, draw, *tri, origin, direction)
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
    use ad_document::{
        Bounds, Cancellation, DocumentId, ImportLimits, NodeId, TextureImage, TextureSampler,
    };
    use std::sync::Arc;

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
        let data = PickData::built(&snapshot, 1);
        let bounds = &data.bounds;
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
            let hit = pick(&snapshot, &data, origin, direction, |_| false).unwrap();
            let through = pick(&snapshot, &data, origin, direction, |i| i == hit.0);
            assert!(
                hit.0 == index || through.is_some_and(|t| t.0 == index),
                "{index}: {hit:?}"
            );
            assert!(hit.1 > 0.);
        }
        // Empty sky is no hit.
        let (origin, direction) = ray(&camera, [0.01, 0.01], w, h).unwrap();
        assert!(pick(&snapshot, &data, origin, direction, |_| false).is_none());
        assert!(pick(&snapshot, &data, origin, direction, |_| true).is_none());
        // Orthographic views give parallel rays.
        camera.set_view(crate::camera::View::Front);
        let (_, a) = ray(&camera, [0.2, 0.5], w, h).unwrap();
        let (_, b) = ray(&camera, [0.8, 0.5], w, h).unwrap();
        assert!(a.dot(b) > 0.9999);
    }

    fn draw(start: u32, count: u32, alpha: f32, cutoff: Option<f32>) -> DrawRange {
        DrawRange {
            node: NodeId(start as usize / 6),
            index_start: start,
            index_count: count,
            base_color: [1., 1., 1., alpha],
            metallic_roughness: [0., 1.],
            color_texture: None,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.,
            occlusion_texture: None,
            occlusion_strength: 1.,
            emissive_factor: [0.; 3],
            emissive_texture: None,
            alpha_cutoff: cutoff,
        }
    }
    fn snapshot(
        positions: Vec<[f32; 3]>,
        uvs: Vec<[f32; 2]>,
        indices: Vec<u32>,
        draws: Vec<DrawRange>,
    ) -> GeometrySnapshot {
        let bounds = Bounds {
            min: [-1.; 3],
            max: [1.; 3],
        };
        GeometrySnapshot {
            document_id: DocumentId(1),
            revision: 1,
            normals: Vec::new(),
            tangents: Vec::new(),
            normal_uvs: Vec::new(),
            occlusion_uvs: Vec::new(),
            emissive_uvs: Vec::new(),
            fallback_tangent_vertices: 0,
            fallback_normal_vertices: 0,
            metallic_roughness_uvs: Vec::new(),
            colors: vec![[1.; 4]; positions.len()],
            positions,
            uvs,
            indices,
            draws,
            bounds,
            source_bounds: bounds,
        }
    }
    /// Two unit quads facing +Z: draw 0 at z = 1 (in front), draw 1 at z = 0.
    fn two_quads(front: DrawRange) -> GeometrySnapshot {
        let mut positions = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();
        for z in [1., 0.] {
            let base = positions.len() as u32;
            for (x, y) in [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)] {
                positions.push([x, y, z]);
                uvs.push([(x + 1.) * 0.5, (1. - y) * 0.5]);
            }
            indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        }
        snapshot(positions, uvs, indices, vec![front, draw(6, 6, 1., None)])
    }
    fn shoot(snapshot: &GeometrySnapshot, x: f32) -> Option<usize> {
        let data = PickData::built(snapshot, 1);
        pick(snapshot, &data, Vec3::new(x, 0.1, 5.), Vec3::NEG_Z, |_| {
            false
        })
        .map(|h| h.0)
    }

    #[test]
    fn a_click_through_a_transparent_mask_reaches_the_object_behind() {
        // MASK with alpha 0 under the cutoff: the renderer discards it.
        assert_eq!(shoot(&two_quads(draw(0, 6, 0., Some(0.5))), 0.), Some(1));
        // Alpha at the cutoff is kept, as in the shader (discard if a < cutoff).
        assert_eq!(shoot(&two_quads(draw(0, 6, 0.5, Some(0.5))), 0.), Some(0));
        // OPAQUE ignores alpha.
        assert_eq!(shoot(&two_quads(draw(0, 6, 0., None)), 0.), Some(0));
        // Vertex colour alpha counts too.
        let mut faded = two_quads(draw(0, 6, 1., Some(0.5)));
        faded.colors[..4].fill([1., 1., 1., 0.2]);
        assert_eq!(shoot(&faded, 0.), Some(1));
    }

    #[test]
    fn a_mask_texture_decides_per_point() {
        // 2×1 texture: left half transparent, right half opaque.
        let texture = |u| {
            Arc::new(Texture {
                image: Arc::new(TextureImage {
                    width: 2,
                    height: 1,
                    rgba: Arc::from([255, 255, 255, 0, 255, 255, 255, 255].as_slice()),
                }),
                sampler: TextureSampler {
                    u,
                    v: Wrap::ClampToEdge,
                    ..Default::default()
                },
            })
        };
        let mut front = draw(0, 6, 1., Some(0.5));
        front.color_texture = Some(texture(Wrap::ClampToEdge));
        let quads = two_quads(front);
        assert_eq!(shoot(&quads, -0.75), Some(1));
        assert_eq!(shoot(&quads, 0.75), Some(0));
        // Wrap modes pick the texel the GPU would.
        assert_eq!(wrap(-1, 4, Wrap::Repeat), 3);
        assert_eq!(wrap(-1, 4, Wrap::MirroredRepeat), 0);
        assert_eq!(wrap(5, 4, Wrap::MirroredRepeat), 2);
        assert_eq!(wrap(9, 4, Wrap::ClampToEdge), 3);
    }

    /// Wavy grid of `n`×`n` cells (2 triangles each) as one draw.
    fn wavy_grid(n: u32) -> GeometrySnapshot {
        let mut positions = Vec::new();
        for j in 0..=n {
            for i in 0..=n {
                let (x, z) = (i as f32 / n as f32 * 2. - 1., j as f32 / n as f32 * 2. - 1.);
                positions.push([x, 0.2 * (x * 9.).sin() * (z * 7.).cos(), z]);
            }
        }
        let mut indices = Vec::new();
        for j in 0..n {
            for i in 0..n {
                let a = j * (n + 1) + i;
                let (b, c, d) = (a + 1, a + n + 1, a + n + 2);
                indices.extend([a, b, d, a, d, c]);
            }
        }
        let count = indices.len() as u32;
        let uvs = vec![[0.; 2]; positions.len()];
        snapshot(positions, uvs, indices, vec![draw(0, count, 1., None)])
    }

    #[test]
    fn the_tree_finds_the_same_hits_as_testing_every_triangle() {
        let grid = wavy_grid(48);
        let tree = PickData::built(&grid, 1);
        let flat = PickData::built(&grid, usize::MAX);
        let mut hits = 0;
        for k in 0..600 {
            let a = k as f32 * 0.618;
            let origin = Vec3::new(a.sin() * 3., 1.5 + (a * 0.7).cos(), (a * 1.3).cos() * 3.);
            let target = Vec3::new((a * 2.1).sin() * 1.1, 0., (a * 1.7).cos() * 1.1);
            let direction = (target - origin).normalize();
            let fast = pick(&grid, &tree, origin, direction, |_| false);
            let slow = pick(&grid, &flat, origin, direction, |_| false);
            match (fast, slow) {
                (Some(f), Some(s)) => {
                    hits += 1;
                    assert!((f.1 - s.1).abs() <= 1e-5 * s.1.max(1.), "{k}: {f:?} {s:?}");
                }
                (None, None) => {}
                other => panic!("{k}: {other:?}"),
            }
        }
        assert!(hits > 300, "{hits}");
        assert!(tree.tree(0).is_some());
        assert!(flat.tree(0).is_none());
    }

    #[test]
    fn the_background_tree_arrives_and_dropping_stops_it() {
        let grid = Arc::new(wavy_grid(48));
        assert!(grid.indices.len() / 3 >= TREE_MIN_TRIANGLES);
        let data = PickData::new(&grid);
        let started = std::time::Instant::now();
        while data.tree(0).is_none() {
            assert!(started.elapsed().as_secs() < 30, "tree never arrived");
            std::thread::yield_now();
        }
        // A raised stop makes the build give up.
        let stop = AtomicBool::new(true);
        assert!(Tree::build(&grid, &grid.draws[0], &stop).is_none());
        let stop = data.stop.clone();
        drop(data);
        assert!(stop.load(Ordering::Relaxed));
    }

    /// Timing of a 1.56-million-triangle draw (run with --release --ignored).
    #[test]
    #[ignore]
    fn heavy_mesh_pick_timing() {
        let grid = wavy_grid(884);
        let triangles = grid.indices.len() / 3;
        let ray = |k: u32| {
            let a = k as f32 * 0.618;
            let origin = Vec3::new(a.sin() * 3., 2., a.cos() * 3.);
            (
                origin,
                (Vec3::new((a * 2.1).sin(), 0., (a * 1.7).cos()) - origin).normalize(),
            )
        };
        let started = std::time::Instant::now();
        let data = PickData::built(&grid, TREE_MIN_TRIANGLES);
        let first = started.elapsed();
        let started = std::time::Instant::now();
        for k in 1..=1000 {
            let (o, d) = ray(k);
            pick(&grid, &data, o, d, |_| false);
        }
        let each = started.elapsed() / 1000;
        let flat = PickData::built(&grid, usize::MAX);
        let started = std::time::Instant::now();
        for k in 1..=20 {
            let (o, d) = ray(k);
            pick(&grid, &flat, o, d, |_| false);
        }
        let brute = started.elapsed() / 20;
        eprintln!("triangles={triangles} build={first:?} pick={each:?} brute={brute:?}");
    }
}
