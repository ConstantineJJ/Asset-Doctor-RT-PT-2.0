//! Morph targets (blend shapes, ADR-0065): per-primitive vertex deltas, the
//! mesh's default weights, per-node weights, and clip channels that animate
//! the weights. A pose adds `Σ weight × delta` to positions (and normals)
//! before skinning. Morph data is view-only: geometry edits stay refused
//! for documents that have it, and saving keeps the file's targets as they
//! are.
use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, ErrorCode, MeshId, NodeId,
    skin_animation::Interpolation,
};
use std::sync::Arc;

fn invalid(message: &str) -> AssetError {
    AssetError::new(ErrorCode::InvalidFormat, message)
}

/// One target: what it adds to each vertex at weight 1.
pub struct MorphTarget {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
}

pub struct MorphPrimitive {
    pub targets: Vec<MorphTarget>,
    /// The mesh's default weights (glTF `mesh.weights`, zeros without).
    pub weights: Arc<[f32]>,
    /// Target names from the mesh's `extras.targetNames`, when present.
    pub names: Arc<[String]>,
}

pub struct Morphs {
    /// Per primitive (document order); None for primitives without targets.
    pub primitives: Vec<Option<Arc<MorphPrimitive>>>,
    /// Per node: its own weights (glTF `node.weights`), overriding the mesh's.
    pub nodes: Vec<Option<Arc<[f32]>>>,
}

impl Morphs {
    pub fn primitive(&self, mesh: MeshId) -> Option<&MorphPrimitive> {
        self.primitives.get(mesh.0).and_then(|p| p.as_deref())
    }
    /// Primitives with targets and the largest target count.
    pub fn summary(&self) -> (usize, usize) {
        let with: Vec<_> = self.primitives.iter().flatten().collect();
        (
            with.len(),
            with.iter().map(|p| p.targets.len()).max().unwrap_or(0),
        )
    }
}

impl AssetDocument {
    pub fn morphs(&self) -> Option<&Morphs> {
        self.morphs.as_deref()
    }

    /// Attaches the file's morph targets after checking them against the
    /// geometry. Nothing is attached when no primitive has targets.
    pub fn with_morphs(mut self, morphs: Morphs, cancel: &Cancellation) -> AssetResult<Self> {
        if morphs.primitives.len() != self.primitives().len()
            || morphs.nodes.len() != self.nodes().len()
        {
            return Err(invalid("Morph targets do not match the loaded document"));
        }
        if morphs.primitives.iter().all(Option::is_none) {
            return Ok(self);
        }
        for (mesh, morph) in self.primitives().iter().zip(&morphs.primitives) {
            let Some(morph) = morph else { continue };
            if morph.targets.is_empty() || morph.weights.len() != morph.targets.len() {
                return Err(invalid("Morph targets and default weights do not match"));
            }
            if morph.weights.iter().any(|w| !w.is_finite()) {
                return Err(invalid("Morph weights must be finite"));
            }
            for target in &morph.targets {
                cancel.check()?;
                let normals_ok = target.normals.as_ref().is_none_or(|n| {
                    n.len() == mesh.positions.len() && n.iter().flatten().all(|v| v.is_finite())
                });
                if target.positions.len() != mesh.positions.len()
                    || target.positions.iter().flatten().any(|v| !v.is_finite())
                    || !normals_ok
                {
                    return Err(invalid(
                        "A morph target must move every vertex of its primitive by a finite amount",
                    ));
                }
            }
        }
        for (node, weights) in self.nodes().iter().zip(&morphs.nodes) {
            let Some(weights) = weights else { continue };
            let expected = node
                .meshes
                .iter()
                .filter_map(|m| morphs.primitive(*m))
                .map(|p| p.targets.len())
                .max();
            if expected != Some(weights.len()) || weights.iter().any(|w| !w.is_finite()) {
                return Err(invalid("Node morph weights do not match its mesh targets"));
            }
        }
        self.morphs = Some(Arc::new(morphs));
        Ok(self)
    }
}

/// The weights a node's primitive uses: animated, else the node's, else the
/// mesh's defaults.
pub(crate) fn weights_for<'a>(
    morphs: &'a Morphs,
    primitive: &'a MorphPrimitive,
    node: NodeId,
    animated: Option<&'a [f32]>,
) -> &'a [f32] {
    animated
        .or_else(|| morphs.nodes.get(node.0).and_then(|w| w.as_deref()))
        .unwrap_or(&primitive.weights)
}

/// Vertex `i` of a primitive with its targets blended in.
pub(crate) fn blend(
    primitive: &MorphPrimitive,
    weights: &[f32],
    i: usize,
    position: [f32; 3],
    normal: Option<[f32; 3]>,
) -> ([f32; 3], Option<[f32; 3]>) {
    let mut p = position;
    let mut n = normal;
    for (target, &w) in primitive.targets.iter().zip(weights) {
        if w == 0. {
            continue;
        }
        let d = target.positions[i];
        for a in 0..3 {
            p[a] += w * d[a];
        }
        if let (Some(n), Some(dn)) = (n.as_mut(), target.normals.as_ref()) {
            for a in 0..3 {
                n[a] += w * dn[i][a];
            }
        }
    }
    (p, n)
}

/// A clip channel animating a node's morph weights: per key `targets`
/// values (cubic spline: in-tangents, values, out-tangents).
pub struct WeightTrack {
    node: NodeId,
    interpolation: Interpolation,
    times: Vec<f32>,
    values: Vec<f32>,
    targets: usize,
    source_channel: Option<usize>,
    edited: bool,
}

impl WeightTrack {
    pub fn new(
        node: NodeId,
        interpolation: Interpolation,
        times: Vec<f32>,
        values: Vec<f32>,
        targets: usize,
        cancel: &Cancellation,
    ) -> AssetResult<Self> {
        let stride = if interpolation == Interpolation::CubicSpline {
            3
        } else {
            1
        };
        if targets == 0
            || times.is_empty()
            || times
                .len()
                .checked_mul(stride)
                .and_then(|n| n.checked_mul(targets))
                != Some(values.len())
        {
            return Err(invalid("Morph weight keys do not match the targets"));
        }
        let mut last = -1.;
        for (i, &time) in times.iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
            }
            if !time.is_finite() || time < 0. || time <= last {
                return Err(invalid(
                    "Animation times must be finite and strictly increasing",
                ));
            }
            last = time;
        }
        if values.iter().any(|v| !v.is_finite()) {
            return Err(invalid("Non-finite morph weight key"));
        }
        Ok(Self {
            node,
            interpolation,
            times,
            values,
            targets,
            source_channel: None,
            edited: false,
        })
    }
    pub fn from_source_channel(mut self, index: usize) -> Self {
        self.source_channel = Some(index);
        self
    }
    pub fn node(&self) -> NodeId {
        self.node
    }
    pub fn interpolation(&self) -> Interpolation {
        self.interpolation
    }
    pub fn times(&self) -> &[f32] {
        &self.times
    }
    pub fn values(&self) -> &[f32] {
        &self.values
    }
    pub fn targets(&self) -> usize {
        self.targets
    }
    pub fn source_channel(&self) -> Option<usize> {
        self.source_channel
    }
    pub fn is_edited(&self) -> bool {
        self.edited
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        size_of::<Self>() + (self.times.capacity() + self.values.capacity()) * size_of::<f32>()
    }

    /// The weights at `time` (held before the first and after the last key).
    pub(crate) fn sample(&self, time: f32) -> Vec<f32> {
        let n = self.targets;
        let cubic = self.interpolation == Interpolation::CubicSpline;
        let at = |key: usize, part: usize| {
            let start = if cubic { (key * 3 + part) * n } else { key * n };
            &self.values[start..start + n]
        };
        let value = |key: usize| at(key, 1);
        let next = self.times.partition_point(|&key| key <= time);
        if next == 0 {
            return value(0).to_vec();
        }
        let prev = next - 1;
        if next == self.times.len() || self.interpolation == Interpolation::Step {
            return value(prev).to_vec();
        }
        let dt = self.times[next] - self.times[prev];
        let u = (time - self.times[prev]) / dt;
        let (a, b) = (value(prev), value(next));
        if cubic {
            let (out, inc) = (at(prev, 2), at(next, 0));
            let (u2, u3) = (u * u, u * u * u);
            (0..n)
                .map(|t| {
                    a[t] * (2. * u3 - 3. * u2 + 1.)
                        + out[t] * ((u3 - 2. * u2 + u) * dt)
                        + b[t] * (-2. * u3 + 3. * u2)
                        + inc[t] * ((u3 - u2) * dt)
                })
                .collect()
        } else {
            (0..n).map(|t| a[t] + (b[t] - a[t]) * u).collect()
        }
    }

    /// The same keys on a clip stretched from `from` to `duration` seconds
    /// (cubic tangents scale with the time).
    pub(crate) fn retimed(
        &self,
        from: f32,
        duration: f32,
        forward: f64,
        cancel: &Cancellation,
    ) -> AssetResult<Self> {
        let mut times = Vec::with_capacity(self.times.len());
        for &time in &self.times {
            let retimed = if time == from {
                duration
            } else {
                (f64::from(time) * forward) as f32
            };
            if times.last().is_some_and(|&p| p >= retimed) {
                return Err(invalid(
                    "Новая длительность сближает ключи до одного времени.",
                ));
            }
            times.push(retimed);
        }
        let n = self.targets;
        let values = if self.interpolation == Interpolation::CubicSpline {
            self.values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    if (i / n) % 3 == 1 {
                        v
                    } else {
                        (f64::from(v) / forward) as f32
                    }
                })
                .collect()
        } else {
            self.values.clone()
        };
        let mut track = Self::new(self.node, self.interpolation, times, values, n, cancel)?;
        track.edited = true;
        track.source_channel = self.source_channel;
        Ok(track)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_sample_step_linear_and_cubic() {
        let c = Cancellation::default();
        let linear = WeightTrack::new(
            NodeId(0),
            Interpolation::Linear,
            vec![0., 1.],
            vec![0., 1., 1., 0.],
            2,
            &c,
        )
        .unwrap();
        assert_eq!(linear.sample(0.5), vec![0.5, 0.5]);
        assert_eq!(linear.sample(-1.), vec![0., 1.]);
        assert_eq!(linear.sample(5.), vec![1., 0.]);
        let step = WeightTrack::new(
            NodeId(0),
            Interpolation::Step,
            vec![0., 1.],
            vec![0., 1.],
            1,
            &c,
        )
        .unwrap();
        assert_eq!(step.sample(0.9), vec![0.]);
        // Cubic with flat tangents eases between the values.
        let cubic = WeightTrack::new(
            NodeId(0),
            Interpolation::CubicSpline,
            vec![0., 1.],
            vec![0., 0., 0., 0., 1., 0.],
            1,
            &c,
        )
        .unwrap();
        assert!((cubic.sample(0.5)[0] - 0.5).abs() < 1e-6);
        assert!(cubic.sample(0.25)[0] < 0.25);
        // Counts must match.
        assert!(
            WeightTrack::new(NodeId(0), Interpolation::Linear, vec![0.], vec![0.], 2, &c).is_err()
        );
        // Retiming keeps the values and stretches the times.
        let slow = linear.retimed(1., 2., 2., &c).unwrap();
        assert_eq!(slow.times(), &[0., 2.]);
        assert_eq!(slow.sample(1.), vec![0.5, 0.5]);
        assert!(slow.is_edited());
    }

    #[test]
    fn blending_adds_weighted_deltas() {
        let p = MorphPrimitive {
            targets: vec![
                MorphTarget {
                    positions: vec![[0., 1., 0.]],
                    normals: Some(vec![[1., 0., 0.]]),
                },
                MorphTarget {
                    positions: vec![[2., 0., 0.]],
                    normals: None,
                },
            ],
            weights: Arc::from([0.5, 0.]),
            names: Arc::from([]),
        };
        let (pos, n) = blend(&p, &[0.5, 0.25], 0, [1., 1., 1.], Some([0., 0., 1.]));
        assert_eq!(pos, [1.5, 1.5, 1.]);
        assert_eq!(n, Some([0.5, 0., 1.]));
    }
}
