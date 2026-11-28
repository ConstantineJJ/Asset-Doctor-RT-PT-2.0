//! RIG-03 (first part): rename a node and add a joint under a joint.
//!
//! The skeleton state — the node list, the rig, the per-node animation
//! bases and the set of renamed nodes — is published as one alternate:
//! Undo swaps the previous state back. Added nodes go to the end of the
//! node list, so the indices of the file's nodes (and every animation
//! channel that targets them) stay the same.
//!
//! A new joint has no vertex weights. It joins every skin its parent belongs
//! to, with the inverse bind matrix `inverse(local) × parent's inverse
//! bind`. That is the parent's bind pose continued by the new local
//! transform, whatever bind convention the file used.
use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode, MeshId, Node,
    NodeId, PreparedRemoval, RemovalPreview, RigInspection, Transform, indices::Alternate,
};
use std::collections::BTreeSet;

/// One skeleton edit.
#[derive(Clone, Debug, PartialEq)]
pub enum JointEdit {
    /// A new name for a node (1–120 characters, no control characters).
    Rename { node: NodeId, name: String },
    /// A new joint under `parent` (a skin joint) at `translation` in the
    /// parent's local space; no rotation, unit scale, no weights.
    Add {
        parent: NodeId,
        name: String,
        translation: [f32; 3],
    },
}

/// The swapped skeleton state.
pub(crate) struct Skeleton {
    pub nodes: Vec<Node>,
    pub rig: Option<RigInspection>,
    pub bases: Vec<Option<Transform>>,
    pub renamed: BTreeSet<usize>,
}

impl Skeleton {
    /// Bytes this state keeps alive in the history.
    pub(crate) fn bytes(&self) -> usize {
        skeleton_bytes(
            &self.nodes,
            self.rig.as_ref(),
            self.bases.len(),
            self.renamed.len(),
        )
    }
}

/// Names, children and joint tables; the shared vertex influences are not
/// counted.
fn skeleton_bytes(
    nodes: &[Node],
    rig: Option<&RigInspection>,
    bases: usize,
    renamed: usize,
) -> usize {
    let nodes: usize = nodes
        .iter()
        .map(|node| {
            std::mem::size_of::<Node>()
                + node.name.len()
                + node.children.len() * std::mem::size_of::<NodeId>()
        })
        .sum();
    let skins: usize = rig.map_or(0, |rig| {
        rig.skins
            .iter()
            .map(|skin| skin.joints.len() * (8 + 64) + skin.name.len())
            .sum::<usize>()
            + rig.node_skins.len() * 16
    });
    nodes + skins + bases * 48 + renamed * 32
}

fn refused(message: &str) -> AssetError {
    AssetError::new(ErrorCode::Unsupported, message.to_string())
}

fn node_name(name: &str) -> AssetResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().take(121).count() > 120 || name.chars().any(char::is_control)
    {
        return Err(refused(
            "Имя сустава должно содержать от 1 до 120 символов без управляющих знаков.",
        ));
    }
    Ok(name.to_owned())
}

impl AssetDocument {
    /// Nodes the file had when it was opened (added joints come after).
    pub fn source_node_count(&self) -> usize {
        self.source_node_count
    }
    /// Nodes renamed since the file was opened.
    pub fn renamed_nodes(&self) -> &BTreeSet<usize> {
        &self.renamed
    }

    /// RIG-03: proposal for one skeleton edit. A rename to the same name is
    /// a verified no-op; an unknown node, an empty or too long name, adding
    /// under a node that is not a skin joint, a zero, non-finite or huge
    /// offset, or the node limit are refused.
    pub fn prepare_joint_edit(
        &self,
        expected: DocumentTag,
        edit: &JointEdit,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.verify_tag(expected, cancel)?;
        let rig = self
            .rig
            .as_ref()
            .ok_or_else(|| refused("Правка суставов доступна для модели со скином."))?;
        let mut nodes = self.parts.nodes.clone();
        let mut rig = rig.clone();
        let mut bases = self.animation_bases.clone();
        let mut renamed = self.renamed.clone();
        cancel.check()?;
        match edit {
            JointEdit::Rename { node, name } => {
                let slot = nodes
                    .get_mut(node.0)
                    .ok_or_else(|| refused("Неизвестный узел"))?;
                let name = node_name(name)?;
                if slot.name == name {
                    return self.skeleton_plan(expected, None);
                }
                slot.name = name;
                renamed.insert(node.0);
            }
            JointEdit::Add {
                parent,
                name,
                translation,
            } => {
                let name = node_name(name)?;
                if parent.0 >= nodes.len() {
                    return Err(refused("Неизвестный родительский сустав"));
                }
                let offset = glam::Vec3::from_array(*translation);
                if !offset.is_finite() || offset.length() <= 1e-6 || offset.length() > 1e6 {
                    return Err(refused(
                        "Смещение нового сустава должно быть конечным, больше нуля и не больше 10⁶.",
                    ));
                }
                if nodes.len() >= crate::ImportLimits::default().nodes {
                    return Err(AssetError::new(
                        ErrorCode::ResourceLimit,
                        "Слишком много узлов",
                    ));
                }
                let index = nodes.len();
                let local = glam::Mat4::from_translation(offset);
                let mut joined = false;
                for (skin, report) in rig.skins.iter_mut().zip(rig.reports.iter_mut()) {
                    cancel.check()?;
                    let Some(slot) = skin.joints.iter().position(|j| *j == *parent) else {
                        continue;
                    };
                    let bind =
                        local.inverse() * glam::Mat4::from_cols_array_2d(&skin.inverse_bind[slot]);
                    if !bind.is_finite() || bind.determinant() == 0. {
                        return Err(refused("Матрица привязки нового сустава вырождена"));
                    }
                    report.unused_joints.push(skin.joints.len());
                    skin.joints.push(NodeId(index));
                    skin.inverse_bind.push(bind.to_cols_array_2d());
                    joined = true;
                }
                if !joined {
                    return Err(refused("Новый сустав добавляется только под сустав скина."));
                }
                nodes[parent.0].children.push(NodeId(index));
                nodes.push(Node {
                    name,
                    local: local.to_cols_array_2d(),
                    children: Vec::new(),
                    meshes: std::sync::Arc::from([]),
                });
                rig.node_skins.push(None);
                if bases.len() == index {
                    bases.push(Some(Transform {
                        translation: *translation,
                        rotation: [0., 0., 0., 1.],
                        scale: [1.; 3],
                    }));
                }
            }
        }
        self.skeleton_plan(
            expected,
            Some(Skeleton {
                nodes,
                rig: Some(rig),
                bases,
                renamed,
            }),
        )
    }

    fn skeleton_plan(
        &self,
        expected: DocumentTag,
        skeleton: Option<Skeleton>,
    ) -> AssetResult<PreparedRemoval> {
        let mesh = self
            .parts
            .meshes
            .first()
            .ok_or_else(|| refused("Правка суставов требует меш"))?;
        let triangles = mesh.indices.len() / 3;
        let vertices = mesh.positions.len();
        let retained = skeleton
            .as_ref()
            .map_or(0, |next| next.bytes().max(self.skeleton_bytes()));
        Ok(PreparedRemoval::new(
            RemovalPreview {
                tag: expected,
                mesh: MeshId(0),
                before_triangles: triangles,
                after_triangles: triangles,
                before_vertices: vertices,
                after_vertices: vertices,
            },
            self.origin.clone(),
            skeleton.map(|s| Alternate::Skeleton(Box::new(s))),
            retained,
        ))
    }

    /// History bytes of the current skeleton state.
    pub(crate) fn skeleton_bytes(&self) -> usize {
        skeleton_bytes(
            &self.parts.nodes,
            self.rig.as_ref(),
            self.animation_bases.len(),
            self.renamed.len(),
        )
    }

    /// Swaps the skeleton state (publication and Undo/Redo).
    pub(crate) fn swap_skeleton(&mut self, skeleton: &mut Skeleton) {
        std::mem::swap(&mut self.parts.nodes, &mut skeleton.nodes);
        std::mem::swap(&mut self.rig, &mut skeleton.rig);
        std::mem::swap(&mut self.animation_bases, &mut skeleton.bases);
        std::mem::swap(&mut self.renamed, &mut skeleton.renamed);
    }
}

/// Checks a proposed skeleton before a view is built on it: the file's
/// nodes keep their places and meshes, every child index is in range, and
/// every skin joint table matches its inverse bind matrices.
pub(crate) fn valid_skeleton(document: &AssetDocument, skeleton: &Skeleton) -> AssetResult<()> {
    let bad = || AssetError::new(ErrorCode::InvalidScene, "Invalid skeleton candidate");
    let nodes = &skeleton.nodes;
    if nodes.len() < document.source_node_count
        || nodes
            .iter()
            .zip(&document.parts.nodes)
            .any(|(a, b)| a.meshes != b.meshes)
        || nodes
            .iter()
            .flat_map(|node| &node.children)
            .any(|child| child.0 >= nodes.len())
    {
        return Err(bad());
    }
    if let Some(rig) = &skeleton.rig
        && (rig.node_skins.len() != nodes.len()
            || rig.skins.iter().any(|skin| {
                skin.joints.len() != skin.inverse_bind.len()
                    || skin.joints.iter().any(|j| j.0 >= nodes.len())
            }))
    {
        return Err(bad());
    }
    Ok(())
}
