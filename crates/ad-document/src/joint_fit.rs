//! RIG-05: where each joint should sit inside the mesh, from the skin
//! weights. Vertices that a joint and its parent joint both pull on (each
//! with at least 10 % weight) form the ring where the limb bends; the
//! weighted centre of that ring, with the smaller of the two weights, is the
//! suggested joint position. Positions are the skinned rest pose.
//!
//! Confidence:
//! - high: at least 16 ring vertices and a shift no longer than the bone;
//! - medium: at least 6 vertices and a shift up to 1.5 bones;
//! - low: anything else.
//!
//! The search only reads the document; applying suggestions is
//! `JointEdit::Fit` (one Undo step, rebind as for `JointEdit::Rest`).
use crate::{AssetDocument, AssetResult, Cancellation, MeshId, NodeId};
use glam::{Mat4, Vec3};
use std::collections::BTreeSet;

/// Least weight of each joint of a pair for a vertex to count.
const MIN_WEIGHT: f32 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitConfidence {
    High,
    Medium,
    Low,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JointFit {
    pub node: NodeId,
    /// World positions at rest: now and suggested.
    pub current: [f32; 3],
    pub suggested: [f32; 3],
    /// Ring vertices behind the suggestion.
    pub samples: usize,
    /// Length of the bone from the parent joint to this one, now.
    pub bone: f32,
    pub confidence: FitConfidence,
}

impl JointFit {
    /// Distance from the current position to the suggested one.
    pub fn shift(&self) -> f32 {
        Vec3::from_array(self.current).distance(Vec3::from_array(self.suggested))
    }
}

fn confidence(samples: usize, shift: f32, bone: f32) -> FitConfidence {
    let bone = bone.max(1e-6);
    if samples >= 16 && shift <= bone {
        FitConfidence::High
    } else if samples >= 6 && shift <= bone * 1.5 {
        FitConfidence::Medium
    } else {
        FitConfidence::Low
    }
}

impl AssetDocument {
    /// RIG-05: one suggestion per skin joint that has a parent joint and a
    /// bend ring (the first skin wins for a joint in several skins).
    pub fn suggest_joint_fits(&self, cancel: &Cancellation) -> AssetResult<Vec<JointFit>> {
        let Some(rig) = self.rig() else {
            return Ok(Vec::new());
        };
        let worlds: Vec<Option<Mat4>> = self
            .view()
            .node_worlds(None, cancel)?
            .into_iter()
            .map(|m| m.map(|m| Mat4::from_cols_array_2d(&m)))
            .collect();
        let nodes = self.nodes();
        let mut parents = vec![None; nodes.len()];
        for (index, node) in nodes.iter().enumerate() {
            for child in &node.children {
                if let Some(slot) = parents.get_mut(child.0) {
                    *slot = Some(index);
                }
            }
        }
        let mut fits: Vec<JointFit> = Vec::new();
        for (s, skin) in rig.skins.iter().enumerate() {
            cancel.check()?;
            // Parent slot: the nearest joint of this skin above each joint.
            let slot_of = |node: usize| skin.joints.iter().position(|j| j.0 == node);
            let parent_slot: Vec<Option<usize>> = skin
                .joints
                .iter()
                .map(|joint| {
                    let mut at = parents.get(joint.0).copied().flatten();
                    let mut steps = 0;
                    while let Some(p) = at {
                        if let Some(slot) = slot_of(p) {
                            return Some(slot);
                        }
                        at = parents[p];
                        steps += 1;
                        if steps > nodes.len() {
                            break;
                        }
                    }
                    None
                })
                .collect();
            let palette = match rig.palette(s, &worlds, cancel) {
                Ok(palette) => palette,
                Err(_) => continue, // a joint outside the scene: no suggestions
            };
            let bound: BTreeSet<usize> = nodes
                .iter()
                .zip(&rig.node_skins)
                .filter(|(_, skin)| **skin == Some(s))
                .flat_map(|(n, _)| n.meshes.iter().map(|m| m.0))
                .collect();
            let mut rings = vec![(Vec3::ZERO, 0f32, 0usize); skin.joints.len()];
            for mesh in bound {
                let Some(influences) = rig.weights(MeshId(mesh)) else {
                    continue;
                };
                let positions = &self.parts.meshes[mesh].positions;
                for (i, (position, influence)) in positions.iter().zip(influences).enumerate() {
                    if i % 4096 == 0 {
                        cancel.check()?;
                    }
                    let mut world = None;
                    for a in 0..4 {
                        let (wa, ja) = (influence.weights[a], usize::from(influence.joints[a]));
                        if wa < MIN_WEIGHT {
                            continue;
                        }
                        for b in 0..4 {
                            let (wb, jb) = (influence.weights[b], usize::from(influence.joints[b]));
                            if a == b || wb < MIN_WEIGHT || parent_slot.get(jb) != Some(&Some(ja)) {
                                continue;
                            }
                            let p = *world.get_or_insert_with(|| {
                                influence
                                    .transform(&palette)
                                    .transform_point3(Vec3::from_array(*position))
                            });
                            let w = wa.min(wb);
                            let ring = &mut rings[jb];
                            ring.0 += p * w;
                            ring.1 += w;
                            ring.2 += 1;
                        }
                    }
                }
            }
            for (slot, (sum, weight, samples)) in rings.into_iter().enumerate() {
                let node = skin.joints[slot];
                let Some(parent) = parent_slot[slot] else {
                    continue;
                };
                if samples == 0 || weight <= 0. || fits.iter().any(|f| f.node == node) {
                    continue;
                }
                let (Some(here), Some(above)) = (worlds[node.0], worlds[skin.joints[parent].0])
                else {
                    continue;
                };
                let current = here.w_axis.truncate();
                let suggested = sum / weight;
                if !suggested.is_finite() {
                    continue;
                }
                let bone = current.distance(above.w_axis.truncate());
                let fit = JointFit {
                    node,
                    current: current.to_array(),
                    suggested: suggested.to_array(),
                    samples,
                    bone,
                    confidence: FitConfidence::Low,
                };
                fits.push(JointFit {
                    confidence: confidence(samples, fit.shift(), bone),
                    ..fit
                });
            }
        }
        Ok(fits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_needs_samples_and_a_plausible_shift() {
        assert_eq!(confidence(20, 0.5, 1.), FitConfidence::High);
        assert_eq!(confidence(20, 1.2, 1.), FitConfidence::Medium);
        assert_eq!(confidence(8, 0.2, 1.), FitConfidence::Medium);
        assert_eq!(confidence(3, 0.1, 1.), FitConfidence::Low);
        assert_eq!(confidence(40, 2., 1.), FitConfidence::Low);
    }
}
