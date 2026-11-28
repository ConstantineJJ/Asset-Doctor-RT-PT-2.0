use super::{
    AssetDocument, AssetError, AssetResult, Cancellation, ErrorCode, ImportLimits, MeshId, NodeId,
};
use glam::Mat4;
use std::collections::HashSet;

#[derive(Clone)]
pub struct Skin {
    pub name: String,
    pub joints: Vec<NodeId>,
    pub inverse_bind: Vec<[[f32; 4]; 4]>,
    pub skeleton: Option<NodeId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Influence {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

pub struct Rig {
    pub skins: Vec<Skin>,
    pub node_skins: Vec<Option<usize>>,
    pub influences: Vec<Option<Vec<Influence>>>,
}

#[derive(Clone)]
pub struct SkinReport {
    pub unused_joints: Vec<usize>,
    pub normalized_vertices: usize,
}

#[derive(Clone)]
pub struct RigInspection {
    pub skins: Vec<Skin>,
    pub reports: Vec<SkinReport>,
    pub(crate) node_skins: Vec<Option<usize>>,
    /// Vertex influences per primitive; skeleton edit states share the
    /// primitives they do not change.
    pub(crate) influences: Vec<Option<std::sync::Arc<[Influence]>>>,
    /// RIG-04: primitives whose JOINTS_0/WEIGHTS_0 the export must write.
    pub(crate) edited_influences: std::collections::BTreeSet<usize>,
}

impl RigInspection {
    /// Same skins, bindings and influences. Joint slots must be equal;
    /// weights may differ by 1e-6 (an export writes rewritten weights as
    /// floats and the import normalises them again).
    pub fn matches(&self, other: &Self, cancel: &Cancellation) -> AssetResult<bool> {
        if self.skins.len() != other.skins.len()
            || self.node_skins != other.node_skins
            || self.influences.len() != other.influences.len()
        {
            return Ok(false);
        }
        for (a, b) in self.skins.iter().zip(&other.skins) {
            cancel.check()?;
            if a.name != b.name
                || a.joints != b.joints
                || a.inverse_bind != b.inverse_bind
                || a.skeleton != b.skeleton
            {
                return Ok(false);
            }
        }
        for (a, b) in self.influences.iter().zip(other.influences.iter()) {
            cancel.check()?;
            match (a, b) {
                (None, None) => {}
                (Some(a), Some(b)) if a.len() == b.len() => {
                    for (a, b) in a.chunks(4096).zip(b.chunks(4096)) {
                        cancel.check()?;
                        let same = a.iter().zip(b).all(|(a, b)| {
                            a.joints == b.joints
                                && a.weights
                                    .iter()
                                    .zip(&b.weights)
                                    .all(|(x, y)| (x - y).abs() <= 1e-6)
                        });
                        if !same {
                            return Ok(false);
                        }
                    }
                }
                _ => return Ok(false),
            }
        }
        Ok(true)
    }
}

fn invalid(message: &str) -> AssetError {
    AssetError::new(ErrorCode::InvalidScene, message)
}

impl AssetDocument {
    pub fn rig(&self) -> Option<&RigInspection> {
        self.rig.as_ref()
    }

    pub fn require_editable(&self) -> AssetResult<()> {
        if self.is_read_only() {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "Изменение геометрии и объектов заблокировано для модели со скином или анимацией.",
            ));
        }
        Ok(())
    }

    pub fn with_rig(
        mut self,
        rig: Rig,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<Self> {
        if self.revision != 0 || self.rig.is_some() {
            return Err(invalid("A rig must be attached on initial import"));
        }
        if rig.skins.is_empty()
            || rig.node_skins.len() != self.nodes().len()
            || rig.influences.len() != self.primitives().len()
        {
            return Err(invalid("Skin bindings do not match the document"));
        }
        let mut used = Vec::with_capacity(rig.skins.len());
        for skin in &rig.skins {
            cancel.check()?;
            if skin.joints.is_empty()
                || skin.joints.len() != skin.inverse_bind.len()
                || skin
                    .skeleton
                    .is_some_and(|node| node.0 >= self.nodes().len())
            {
                return Err(invalid("Invalid skin joint or inverse bind count"));
            }
            let mut unique = HashSet::new();
            for (&joint, matrix) in skin.joints.iter().zip(&skin.inverse_bind) {
                cancel.check()?;
                let m = Mat4::from_cols_array_2d(matrix);
                if joint.0 >= self.nodes().len()
                    || !unique.insert(joint.0)
                    || !m.is_finite()
                    || m.row(3) != glam::Vec4::W
                    || m.determinant() == 0.
                    || !m.inverse().is_finite()
                {
                    return Err(invalid(
                        "Invalid or duplicate skin joint or inverse bind matrix",
                    ));
                }
            }
            used.push(vec![false; skin.joints.len()]);
        }

        let mut primitive_stats = Vec::with_capacity(self.primitives().len());
        let mut influences = rig.influences;
        for (mesh, weights) in self.primitives().iter().zip(&mut influences) {
            cancel.check()?;
            let Some(weights) = weights else {
                primitive_stats.push(None);
                continue;
            };
            if weights.len() != mesh.positions.len() {
                return Err(invalid("Skin influences do not match POSITION count"));
            }
            let mut active = HashSet::new();
            let mut max_slot = 0;
            let mut normalized = 0;
            for (i, influence) in weights.iter_mut().enumerate() {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                if influence.weights.iter().any(|w| !(0.0..=1.0).contains(w)) {
                    return Err(invalid("Skin weights must be finite and between 0 and 1"));
                }
                let sum: f32 = influence.weights.iter().sum();
                if sum <= f32::MIN_POSITIVE {
                    return Err(invalid("A skinned vertex has no positive weights"));
                }
                normalized += usize::from((sum - 1.).abs() > 0.0001);
                for (&slot, weight) in influence.joints.iter().zip(&mut influence.weights) {
                    max_slot = max_slot.max(slot);
                    if *weight > 0. {
                        active.insert(slot);
                        *weight /= sum;
                    }
                }
            }
            primitive_stats.push(Some((max_slot, active, normalized)));
        }

        let mut pairs = HashSet::new();
        let mut checked = HashSet::new();
        let mut normalized_by_skin = vec![0usize; rig.skins.len()];
        for (node, skin) in self.nodes().iter().zip(&rig.node_skins) {
            let Some(skin) = *skin else { continue };
            cancel.check()?;
            let slots = used
                .get_mut(skin)
                .ok_or_else(|| invalid("Unknown skin binding"))?;
            if node.meshes.is_empty() {
                return Err(invalid("Skin binding has no mesh"));
            }
            if !checked.insert((skin, node.meshes.as_ptr(), node.meshes.len())) {
                continue;
            }
            for mesh in node.meshes.iter() {
                cancel.check()?;
                if !pairs.insert((skin, mesh.0)) {
                    continue;
                }
                if pairs.len() > limits.draws {
                    return Err(AssetError::new(
                        ErrorCode::ResourceLimit,
                        "Too many skin bindings",
                    ));
                }
                let Some((max_slot, active, normalized)) = &primitive_stats[mesh.0] else {
                    return Err(invalid("A skinned primitive is missing JOINTS_0/WEIGHTS_0"));
                };
                if usize::from(*max_slot) >= slots.len() {
                    return Err(invalid("Joint index is outside the skin's joint table"));
                }
                for slot in active {
                    slots[usize::from(*slot)] = true;
                }
                normalized_by_skin[skin] += normalized;
            }
        }
        let reports = used
            .into_iter()
            .zip(normalized_by_skin)
            .map(|(used, normalized_vertices)| SkinReport {
                unused_joints: used
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, used)| (!used).then_some(i))
                    .collect(),
                normalized_vertices,
            })
            .collect();
        let rig = RigInspection {
            skins: rig.skins,
            reports,
            influences: influences
                .into_iter()
                .map(|weights| weights.map(std::sync::Arc::from))
                .collect(),
            edited_influences: std::collections::BTreeSet::new(),
            node_skins: rig.node_skins,
        };
        // RIG-06: the file's skeleton, for «reset the rig» (clips are
        // attached before the rig; influences are shared, not copied).
        self.pristine = Some(std::sync::Arc::new(crate::joint_edit::Pristine {
            nodes: self.parts.nodes.clone(),
            rig: rig.clone(),
            bases: self.animation_bases.clone(),
            clips: self.animation_clips.clone(),
        }));
        self.rig = Some(rig);
        Ok(self)
    }
}

impl RigInspection {
    pub(crate) fn palette(
        &self,
        skin: usize,
        world: &[Option<Mat4>],
        cancel: &Cancellation,
    ) -> AssetResult<Vec<Mat4>> {
        let skin = &self.skins[skin];
        skin.joints
            .iter()
            .zip(&skin.inverse_bind)
            .map(|(joint, bind)| {
                cancel.check()?;
                let world = world[joint.0]
                    .ok_or_else(|| invalid("A skin joint is outside the active scene"))?;
                let matrix = world * Mat4::from_cols_array_2d(bind);
                if !matrix.is_finite() {
                    return Err(invalid("A skin joint matrix overflowed"));
                }
                Ok(matrix)
            })
            .collect()
    }

    /// Vertex influences of a primitive (normalised weights).
    pub fn primitive_influences(&self, mesh: MeshId) -> Option<&[Influence]> {
        self.influences.get(mesh.0)?.as_deref()
    }
    /// RIG-04: primitives whose JOINTS_0/WEIGHTS_0 changed since opening.
    pub fn edited_influences(&self) -> &std::collections::BTreeSet<usize> {
        &self.edited_influences
    }

    pub(crate) fn skin_at(&self, node: NodeId) -> Option<usize> {
        self.node_skins[node.0]
    }

    pub(crate) fn weights(&self, mesh: MeshId) -> Option<&[Influence]> {
        self.influences[mesh.0].as_deref()
    }
}

impl Influence {
    pub(crate) fn transform(&self, palette: &[Mat4]) -> Mat4 {
        self.joints
            .iter()
            .zip(self.weights)
            .filter(|(_, weight)| *weight > 0.)
            .fold(Mat4::ZERO, |sum, (&joint, weight)| {
                sum + palette[usize::from(joint)] * weight
            })
    }
}
