//! GPU skinning data (ADR-0066). The layout lists every snapshot vertex in
//! the snapshot's own order, with its source position, normal and tangent
//! and up to four matrix slots with weights. A static instance uses its
//! node's world slot with weight 1; a skinned one its skin's palette. Per
//! frame only the matrices change: node worlds (slots 0..nodes) followed by
//! each skin's palette. `skin_reference` is the shader's math on the CPU.
use crate::{
    AssetResult, Cancellation, DocumentId, DocumentView, ImportLimits, NodeId,
    skin_animation::ChannelValue,
};
use glam::{Mat3, Mat4, Vec3, Vec4};

/// One vertex for the skinning shader (std430, 80 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinVertex {
    /// xyz: source position.
    pub position: [f32; 4],
    /// xyz: source normal; w = 1 when the mesh has normals.
    pub normal: [f32; 4],
    /// Source tangent; w = 0 when the mesh has none (else ±1).
    pub tangent: [f32; 4],
    pub slots: [u32; 4],
    pub weights: [f32; 4],
}

pub struct SkinLayout {
    pub document_id: DocumentId,
    pub revision: u64,
    pub vertices: Vec<SkinVertex>,
    /// Matrices per frame: node worlds, then every skin's palette.
    pub matrices: usize,
    nodes: usize,
    /// First palette slot per skin, when an instance uses that skin.
    skins: Vec<Option<usize>>,
}

impl DocumentView<'_> {
    /// The GPU skinning layout of this view, or None when the CPU pose is
    /// needed (morph targets change the source vertices).
    pub fn skin_layout(
        &self,
        limits: ImportLimits,
        cancel: &Cancellation,
    ) -> AssetResult<Option<SkinLayout>> {
        let document = self.document();
        if document.morphs().is_some() {
            return Ok(None);
        }
        let nodes = self.nodes();
        let edits = self.node_edits();
        let rig = self.rig();
        let mut base = nodes.len();
        let mut skins = Vec::new();
        let mut palette_start = Vec::new();
        if let Some(rig) = rig {
            for skin in &rig.skins {
                palette_start.push(base);
                base += skin.joints.len();
                skins.push(None);
            }
        }
        let mut visited = vec![false; nodes.len()];
        let mut pending: Vec<NodeId> = document
            .roots()
            .iter()
            .rev()
            .filter(|id| !edits.is_detached(**id))
            .copied()
            .collect();
        let mut vertices = Vec::new();
        while let Some(id) = pending.pop() {
            cancel.check()?;
            if std::mem::replace(&mut visited[id.0], true) {
                return Ok(None); // the snapshot refuses this scene
            }
            let node = &nodes[id.0];
            for mesh_id in node.meshes.iter() {
                let mesh = self.primitive(*mesh_id);
                let skinned = rig.and_then(|rig| {
                    let skin = rig.skin_at(id)?;
                    Some((skin, rig.weights(*mesh_id)?))
                });
                if let Some((skin, _)) = skinned {
                    skins[skin] = Some(palette_start[skin]);
                }
                let count = mesh.positions.len();
                if vertices.len() + count > limits.snapshot_bytes / size_of::<SkinVertex>() {
                    return Ok(None);
                }
                vertices
                    .try_reserve(count)
                    .map_err(|_| crate::indices::budget())?;
                for i in 0..count {
                    if i % 4096 == 0 {
                        cancel.check()?;
                    }
                    let [x, y, z] = mesh.positions[i];
                    let normal = mesh.normals.as_ref().map_or([0.; 4], |n| {
                        let [a, b, c] = n[i];
                        [a, b, c, 1.]
                    });
                    let tangent = mesh.tangents.as_ref().map_or([0.; 4], |t| t[i]);
                    let (slots, weights) = match skinned {
                        Some((skin, influences)) => {
                            let influence = &influences[i];
                            (
                                influence
                                    .joints
                                    .map(|j| (palette_start[skin] + usize::from(j)) as u32),
                                influence.weights,
                            )
                        }
                        None => ([id.0 as u32, 0, 0, 0], [1., 0., 0., 0.]),
                    };
                    vertices.push(SkinVertex {
                        position: [x, y, z, 1.],
                        normal,
                        tangent,
                        slots,
                        weights,
                    });
                }
            }
            pending.extend(
                node.children
                    .iter()
                    .rev()
                    .filter(|child| !edits.is_detached(**child)),
            );
        }
        if vertices.is_empty() {
            return Ok(None);
        }
        Ok(Some(SkinLayout {
            document_id: document.tag().document,
            revision: self.document().revision(),
            vertices,
            matrices: base,
            nodes: nodes.len(),
            skins,
        }))
    }

    /// The matrices of one pose for `layout` (column-major): node worlds in
    /// that pose, then the palettes of the skins in use.
    pub fn pose_matrices(
        &self,
        layout: &SkinLayout,
        pose: Option<(usize, f32, Option<ChannelValue>)>,
        cancel: &Cancellation,
    ) -> AssetResult<Vec<[[f32; 4]; 4]>> {
        if layout.nodes != self.nodes().len() {
            return Err(crate::AssetError::new(
                crate::ErrorCode::Conflict,
                "The skinning layout belongs to another skeleton",
            ));
        }
        let worlds: Vec<Option<Mat4>> = self
            .node_worlds(pose, cancel)?
            .into_iter()
            .map(|w| w.map(|m| Mat4::from_cols_array_2d(&m)))
            .collect();
        let mut matrices = vec![Mat4::IDENTITY.to_cols_array_2d(); layout.matrices];
        for (slot, world) in matrices.iter_mut().zip(&worlds) {
            if let Some(world) = world {
                *slot = world.to_cols_array_2d();
            }
        }
        if let Some(rig) = self.rig() {
            for (skin, start) in layout.skins.iter().enumerate() {
                let Some(start) = start else { continue };
                for (k, m) in rig.palette(skin, &worlds, cancel)?.into_iter().enumerate() {
                    matrices[start + k] = m.to_cols_array_2d();
                }
            }
        }
        Ok(matrices)
    }
}

/// The skinning shader's result for one vertex: position, normal (zero
/// without one or for a degenerate matrix) and tangent (zero without one).
pub fn skin_reference(
    v: &SkinVertex,
    matrices: &[[[f32; 4]; 4]],
) -> ([f32; 3], [f32; 3], [f32; 4]) {
    let mut m = Mat4::ZERO;
    for k in 0..4 {
        if v.weights[k] > 0. {
            m += Mat4::from_cols_array_2d(&matrices[v.slots[k] as usize]) * v.weights[k];
        }
    }
    let p = m * Vec4::new(v.position[0], v.position[1], v.position[2], 1.);
    let linear = Mat3::from_mat4(m);
    let det = linear.determinant();
    let (mut n, mut t) = (Vec3::ZERO, Vec4::ZERO);
    if det != 0. && det.is_finite() {
        if v.normal[3] > 0.5 {
            n = (linear.inverse().transpose() * Vec3::new(v.normal[0], v.normal[1], v.normal[2]))
                .normalize_or_zero();
        }
        if v.tangent[3] != 0. && n != Vec3::ZERO {
            let raw =
                (linear * Vec3::new(v.tangent[0], v.tangent[1], v.tangent[2])).normalize_or_zero();
            let projected = raw - n * n.dot(raw);
            if projected.length_squared() >= 1e-12 {
                t = projected.normalize().extend(v.tangent[3] * det.signum());
            }
        }
    }
    (p.truncate().to_array(), n.to_array(), t.to_array())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_blends_matrices_and_keeps_normals_unit() {
        let v = SkinVertex {
            position: [1., 0., 0., 1.],
            normal: [0., 1., 0., 1.],
            tangent: [1., 0., 0., 1.],
            slots: [0, 1, 0, 0],
            weights: [0.5, 0.5, 0., 0.],
        };
        let matrices = [
            Mat4::from_translation(Vec3::new(0., 2., 0.)).to_cols_array_2d(),
            Mat4::from_scale(Vec3::new(2., 1., 1.)).to_cols_array_2d(),
        ];
        let (p, n, t) = skin_reference(&v, &matrices);
        // Half the offset identity, half the stretch: x 1.5, y 1.
        assert_eq!(p, [1.5, 1., 0.]);
        assert_eq!(n, [0., 1., 0.]);
        assert_eq!(t, [1., 0., 0., 1.]);
        // Half a mirror flattens x: no normal or tangent direction is left.
        let flat = [
            matrices[0],
            Mat4::from_scale(Vec3::new(-1., 1., 1.)).to_cols_array_2d(),
        ];
        let (p, n, t) = skin_reference(&v, &flat);
        assert_eq!(p, [0., 1., 0.]);
        assert_eq!((n, t), ([0.; 3], [0.; 4]));
        // No normal in the mesh: no normal, no tangent.
        let bare = SkinVertex {
            normal: [0.; 4],
            ..v
        };
        let (_, n, t) = skin_reference(&bare, &matrices);
        assert_eq!((n, t), ([0.; 3], [0.; 4]));
    }
}
