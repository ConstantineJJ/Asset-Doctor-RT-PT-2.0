//! ADR-0048 object edits: a scene node gets a new local transform (move,
//! turn, scale by the object gizmo or a left-button drag) or is taken out of
//! the scene (delete). The document keeps the whole edit state; each edit
//! publishes a new state and Undo restores the previous one. Primitive data,
//! names and the node list never change; a taken-out node stays in the file
//! as an unused node.
use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode, MeshId, NodeId,
    PreparedRemoval, RemovalPreview, indices::Alternate,
};
use std::collections::{BTreeMap, BTreeSet};

/// One object edit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeOp {
    /// Replace the node's local transform (column-major matrix).
    Local(NodeId, [[f32; 4]; 4]),
    /// Take the node (and so everything under it) out of the scene.
    Detach(NodeId),
}

/// All object edits of a document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeEdits {
    pub locals: BTreeMap<usize, [[f32; 4]; 4]>,
    pub detached: BTreeSet<usize>,
}
impl NodeEdits {
    pub fn is_empty(&self) -> bool {
        self.locals.is_empty() && self.detached.is_empty()
    }
    /// Effective local transform of a node.
    pub fn local(&self, document: &AssetDocument, node: NodeId) -> [[f32; 4]; 4] {
        self.locals
            .get(&node.0)
            .copied()
            .unwrap_or(document.parts.nodes[node.0].local)
    }
    pub fn is_detached(&self, node: NodeId) -> bool {
        self.detached.contains(&node.0)
    }
    /// A local transform the file can store: finite and decomposable into
    /// translation, rotation and positive scale without shear.
    pub fn valid_local(matrix: [[f32; 4]; 4]) -> bool {
        let m = glam::Mat4::from_cols_array_2d(&matrix);
        if !m.is_finite() || m.row(3) != glam::Vec4::W {
            return false;
        }
        let (scale, rotation, translation) = m.to_scale_rotation_translation();
        if scale.min_element().is_nan() || scale.min_element() <= 1e-8 || !rotation.is_finite() {
            return false;
        }
        let back = glam::Mat4::from_scale_rotation_translation(scale, rotation, translation);
        let size = m.abs().to_cols_array().into_iter().fold(1f32, f32::max);
        m.abs_diff_eq(back, 1e-4 * size)
    }
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.locals.len() * 96 + self.detached.len() * 32
    }
}

impl AssetDocument {
    pub fn node_edits(&self) -> &NodeEdits {
        &self.node_edits
    }
    /// Scene roots, without nodes taken out by object edits.
    pub fn scene_roots(&self) -> Vec<NodeId> {
        self.parts
            .roots
            .iter()
            .copied()
            .filter(|n| !self.node_edits.is_detached(*n))
            .collect()
    }
    /// Children of a node, without nodes taken out by object edits.
    pub fn scene_children(&self, node: NodeId) -> Vec<NodeId> {
        self.parts.nodes[node.0]
            .children
            .iter()
            .copied()
            .filter(|n| !self.node_edits.is_detached(*n))
            .collect()
    }
    /// ADR-0048: proposal for one object edit on top of the current edits.
    /// A transform equal to the current one is a verified no-op; an unknown
    /// node, a matrix the file cannot store or taking out the last visible
    /// node are refused.
    pub fn prepare_node_op(
        &self,
        expected: DocumentTag,
        op: NodeOp,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.require_editable()?;
        self.verify_tag(expected, cancel)?;
        let refused = |m: &str| AssetError::new(ErrorCode::Unsupported, m.to_string());
        let mut edits = self.node_edits.clone();
        match op {
            NodeOp::Local(node, matrix) => {
                if node.0 >= self.parts.nodes.len() {
                    return Err(refused("Unknown object"));
                }
                if !NodeEdits::valid_local(matrix) {
                    return Err(refused(
                        "The new object transform cannot be stored (shear or invalid scale)",
                    ));
                }
                if matrix == self.parts.nodes[node.0].local {
                    edits.locals.remove(&node.0);
                } else {
                    edits.locals.insert(node.0, matrix);
                }
            }
            NodeOp::Detach(node) => {
                if node.0 >= self.parts.nodes.len() {
                    return Err(refused("Unknown object"));
                }
                edits.detached.insert(node.0);
                // Something must stay in the scene.
                if !Self::shallow_edits(self, &edits).iter().any(|shown| *shown) {
                    return Err(refused("The last object of the scene cannot be deleted"));
                }
            }
        }
        let first = self
            .parts
            .meshes
            .first()
            .ok_or_else(|| refused("A model without primitives has no objects to edit"))?;
        let triangles = first.indices.len() / 3;
        let bytes = edits.bytes();
        let replacement = (edits != self.node_edits).then(|| Alternate::Nodes(Box::new(edits)));
        Ok(PreparedRemoval::new(
            RemovalPreview {
                tag: expected,
                mesh: MeshId(0),
                before_triangles: triangles,
                after_triangles: triangles,
                before_vertices: first.positions.len(),
                after_vertices: first.positions.len(),
            },
            self.origin.clone(),
            replacement,
            bytes,
        ))
    }
    /// For each node: does it reach a primitive in the scene with `edits`?
    fn shallow_edits(document: &AssetDocument, edits: &NodeEdits) -> Vec<bool> {
        let nodes = &document.parts.nodes;
        let mut shown = vec![false; nodes.len()];
        let mut seen = vec![false; nodes.len()];
        let mut pending: Vec<usize> = document
            .parts
            .roots
            .iter()
            .map(|n| n.0)
            .filter(|n| !edits.detached.contains(n))
            .collect();
        while let Some(n) = pending.pop() {
            if n >= nodes.len() || seen[n] {
                continue;
            }
            seen[n] = true;
            shown[n] = !nodes[n].meshes.is_empty();
            pending.extend(
                nodes[n]
                    .children
                    .iter()
                    .map(|c| c.0)
                    .filter(|c| !edits.detached.contains(c)),
            );
        }
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locals_must_be_storable_transforms() {
        let trs = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(1., 2., 3.),
            glam::Quat::from_rotation_y(0.7),
            glam::Vec3::new(4., 5., 6.),
        );
        assert!(NodeEdits::valid_local(trs.to_cols_array_2d()));
        let shear = glam::Mat4::from_cols_array_2d(&[
            [1., 0.5, 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
        ]);
        assert!(!NodeEdits::valid_local(shear.to_cols_array_2d()));
        let mut bad = glam::Mat4::IDENTITY.to_cols_array_2d();
        bad[0][0] = f32::NAN;
        assert!(!NodeEdits::valid_local(bad));
        assert!(!NodeEdits::valid_local(glam::Mat4::ZERO.to_cols_array_2d()));
        let mut projective = glam::Mat4::IDENTITY.to_cols_array_2d();
        projective[0][3] = 0.5;
        assert!(!NodeEdits::valid_local(projective));
    }
}
