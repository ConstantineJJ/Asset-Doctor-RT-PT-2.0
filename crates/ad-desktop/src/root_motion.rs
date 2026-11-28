//! ANIM-05: root motion of a clip. A joint's world position is sampled over
//! the clip (30 per second, 2–901 samples). The summary gives the
//! displacement, the distance on the floor, the path length, the average
//! speed, the turn around Y and the height range; the viewport draws the
//! path.
use ad_document::{AssetDocument, Cancellation, NodeId};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    /// End minus start, world units (glTF: metres).
    pub delta: [f32; 3],
    /// |delta| on the floor (X, Z).
    pub horizontal: f32,
    /// Sum of the sampled steps.
    pub length: f32,
    /// horizontal / duration (0 for an empty clip).
    pub speed: f32,
    /// Turn around Y from start to end, degrees in (-180, 180].
    pub yaw: f32,
    pub min_y: f32,
    pub max_y: f32,
}

/// The Rigging card's choices and the last measurement.
#[derive(Clone, Debug, Default)]
pub struct PathState {
    pub motion: Option<Arc<RootMotion>>,
    /// Draw the path in the viewport (Rigging tab).
    pub show: bool,
    /// Measure the selected joint instead of the skeleton's root.
    pub selected_joint: bool,
}

/// One measured path: what it was measured on and the samples.
#[derive(Clone, Debug, PartialEq)]
pub struct RootMotion {
    pub document: u64,
    pub revision: u64,
    pub clip: usize,
    pub node: NodeId,
    pub times: Vec<f32>,
    pub points: Vec<[f32; 3]>,
    pub summary: Summary,
}

impl RootMotion {
    /// Position on the path at `time` (linear between samples).
    pub fn at(&self, time: f32) -> Option<[f32; 3]> {
        let last = self.times.len().checked_sub(1)?;
        let i = self
            .times
            .partition_point(|&t| t <= time)
            .clamp(1, last.max(1));
        if last == 0 {
            return self.points.first().copied();
        }
        let (t0, t1) = (self.times[i - 1], self.times[i]);
        let k = if t1 > t0 {
            ((time - t0) / (t1 - t0)).clamp(0., 1.)
        } else {
            0.
        };
        let (a, b) = (self.points[i - 1], self.points[i]);
        Some([0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * k))
    }
}

/// Yaw (degrees) of a world matrix: the direction its local Z axis points
/// to on the floor.
fn yaw(world: &[[f32; 4]; 4]) -> f32 {
    let [x, _, z, _] = world[2];
    x.atan2(z).to_degrees()
}

pub fn summarize(times: &[f32], worlds: &[[[f32; 4]; 4]]) -> Summary {
    let points: Vec<[f32; 3]> = worlds.iter().map(|m| [m[3][0], m[3][1], m[3][2]]).collect();
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Summary::default();
    };
    let delta = [0, 1, 2].map(|c| last[c] - first[c]);
    let horizontal = delta[0].hypot(delta[2]);
    let length = points
        .windows(2)
        .map(|w| {
            let d = [0, 1, 2].map(|c| w[1][c] - w[0][c]);
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .sum();
    let duration = times.last().copied().unwrap_or(0.) - times.first().copied().unwrap_or(0.);
    let mut turn = yaw(&worlds[worlds.len() - 1]) - yaw(&worlds[0]);
    while turn > 180. {
        turn -= 360.;
    }
    while turn <= -180. {
        turn += 360.;
    }
    let (min_y, max_y) = points
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[1]), hi.max(p[1]))
        });
    Summary {
        delta,
        horizontal,
        length,
        speed: if duration > 0. {
            horizontal / duration
        } else {
            0.
        },
        yaw: turn,
        min_y,
        max_y,
    }
}

/// Samples for a clip: 30 per second, at least 2, at most 901.
fn sample_count(duration: f32) -> usize {
    ((duration.max(0.) * 30.).ceil() as usize + 1).clamp(2, 901)
}

/// Measures `node` over `clip` in the current document (placement and
/// object edits included).
pub fn measure(document: &AssetDocument, node: NodeId, clip: usize) -> Option<Arc<RootMotion>> {
    let duration = document.animation_clips().get(clip)?.duration;
    let path = document
        .view()
        .node_path(node, clip, sample_count(duration), &Cancellation::default())
        .ok()?;
    let times: Vec<f32> = path.iter().map(|(t, _)| *t).collect();
    let worlds: Vec<[[f32; 4]; 4]> = path.iter().map(|(_, m)| *m).collect();
    Some(Arc::new(RootMotion {
        document: document.id().0,
        revision: document.revision(),
        clip,
        node,
        summary: summarize(&times, &worlds),
        points: worlds.iter().map(|m| [m[3][0], m[3][1], m[3][2]]).collect(),
        times,
    }))
}

/// The skeleton's root: the first skin's `skeleton` node when it is a
/// joint, else its first joint without a joint ancestor; without a skin,
/// the first animated node of the clip.
pub fn root_joint(document: &AssetDocument, clip: usize) -> Option<NodeId> {
    let Some(skin) = document.rig().and_then(|rig| rig.skins.first()) else {
        return document
            .animation_clips()
            .get(clip)?
            .tracks()
            .first()
            .map(|track| track.node());
    };
    let nodes = document.nodes();
    let mut parents = vec![None; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        for child in &node.children {
            if let Some(slot) = parents.get_mut(child.0) {
                *slot = Some(index);
            }
        }
    }
    let is_joint = |n: usize| skin.joints.iter().any(|j| j.0 == n);
    if let Some(root) = skin.skeleton.filter(|r| is_joint(r.0)) {
        return Some(root);
    }
    skin.joints.iter().copied().find(|joint| {
        let mut at = parents.get(joint.0).copied().flatten();
        let mut steps = 0;
        while let Some(p) = at {
            if is_joint(p) {
                return false;
            }
            at = parents[p];
            steps += 1;
            if steps > nodes.len() {
                break;
            }
        }
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, y: f32, z: f32, yaw_degrees: f32) -> [[f32; 4]; 4] {
        let (s, c) = yaw_degrees.to_radians().sin_cos();
        [
            [c, 0., -s, 0.],
            [0., 1., 0., 0.],
            [s, 0., c, 0.],
            [x, y, z, 1.],
        ]
    }

    #[test]
    fn a_walk_forward_with_a_turn_is_measured() {
        let times = [0., 0.5, 1., 1.5, 2.];
        let worlds = [
            at(0., 1., 0., 0.),
            at(0., 1.1, 1., 10.),
            at(0., 1., 2., 30.),
            at(0., 1.1, 3., 60.),
            at(0., 1., 4., 90.),
        ];
        let s = summarize(&times, &worlds);
        assert_eq!(s.delta, [0., 0., 4.]);
        assert!((s.horizontal - 4.).abs() < 1e-5);
        assert!((s.speed - 2.).abs() < 1e-5);
        assert!(s.length > 4. && s.length < 4.1, "{}", s.length);
        assert!((s.yaw - 90.).abs() < 1e-3, "{}", s.yaw);
        assert_eq!((s.min_y, s.max_y), (1., 1.1));
        // A turn past 180° wraps to the short way.
        let s = summarize(&[0., 1.], &[at(0., 0., 0., 170.), at(0., 0., 0., -170.)]);
        assert!((s.yaw - 20.).abs() < 1e-3, "{}", s.yaw);
        assert_eq!(summarize(&[], &[]), Summary::default());
    }

    #[test]
    fn samples_and_positions_between_them() {
        assert_eq!(sample_count(0.), 2);
        assert_eq!(sample_count(1.), 31);
        assert_eq!(sample_count(1000.), 901);
        let motion = RootMotion {
            document: 1,
            revision: 0,
            clip: 0,
            node: NodeId(0),
            times: vec![0., 1.],
            points: vec![[0., 0., 0.], [2., 0., 4.]],
            summary: Summary::default(),
        };
        assert_eq!(motion.at(0.5), Some([1., 0., 2.]));
        assert_eq!(motion.at(-1.), Some([0., 0., 0.]));
        assert_eq!(motion.at(5.), Some([2., 0., 4.]));
    }
}
