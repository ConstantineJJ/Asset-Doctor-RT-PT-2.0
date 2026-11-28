//! RIG-03 / RIG-04: rename a node, add a joint under a joint, delete a
//! joint with everything under it.
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
//!
//! Deleting a joint takes it and its subtree out of the scene; the nodes
//! stay in the file unused (like ADR-0048), so no index moves. Their skin
//! slots leave the joint tables, every vertex weight they had goes to the
//! nearest remaining joint above, the JOINTS_0 slots are renumbered, and
//! their animation channels leave the clips.
use crate::{
    AnimationClip, AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode,
    Influence, MeshId, Node, NodeId, PreparedRemoval, RemovalPreview, RigInspection, Transform,
    indices::Alternate,
};
use std::{collections::BTreeSet, sync::Arc};

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
    /// RIG-04: take a skin joint and everything under it out of the scene;
    /// its weights go to the nearest remaining joint above.
    Delete { node: NodeId },
    /// RIG-03: a new rest position of a skin joint (its translation in the
    /// parent's space; rotation and scale stay). Its own and its
    /// descendants' inverse binds are rebound, so the mesh stays in place
    /// at rest and only the skeleton moves.
    ///
    /// RIG-02 child behaviour: with `hold`, the joint's direct children
    /// keep their rest world positions (their translations take the
    /// opposite move), so only this joint moves; otherwise they follow.
    Rest {
        node: NodeId,
        translation: [f32; 3],
        hold: bool,
    },
    /// RIG-05: move joints to world positions at rest (fitting to the
    /// mesh), parents first, each with the rebind of `Rest`; one Undo step.
    Fit { targets: Vec<(NodeId, [f32; 3])> },
    /// RIG-06: the skeleton back to the file (names, rest poses, joints,
    /// binds, weights). Clips keep their keys: channels of deleted joints
    /// come back from the file, channels of added joints go, bases follow
    /// the restored rests.
    Reset,
}

/// RIG-06: the file's skeleton and clips, kept from the import.
pub(crate) struct Pristine {
    pub nodes: Vec<Node>,
    pub rig: RigInspection,
    pub bases: Vec<Option<Transform>>,
    pub clips: Vec<Arc<AnimationClip>>,
}

/// The swapped skeleton state.
pub(crate) struct Skeleton {
    pub nodes: Vec<Node>,
    pub rig: Option<RigInspection>,
    pub bases: Vec<Option<Transform>>,
    pub renamed: BTreeSet<usize>,
    /// RIG-04 drops the channels of deleted joints.
    pub clips: Vec<Arc<AnimationClip>>,
    /// Joints were added, deleted or moved (the export rewrites the skeleton).
    pub restructured: bool,
    /// Nodes with a new rest transform.
    pub rested: BTreeSet<usize>,
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
        let mut clips = self.copy_clip_list(0)?;
        let restructured = self.restructured || !matches!(edit, JointEdit::Rename { .. });
        let mut rested = self.rested.clone();
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
            JointEdit::Delete { node } => {
                delete_joint(&mut nodes, &mut rig, &mut clips, *node, cancel)?;
            }
            JointEdit::Reset => {
                if !self.restructured && self.renamed.is_empty() && self.rested.is_empty() {
                    return self.skeleton_plan(expected, None);
                }
                let pristine = self
                    .pristine
                    .as_ref()
                    .ok_or_else(|| refused("Исходный скелет файла не сохранён"))?;
                reset_clips(self, pristine, &mut clips, cancel)?;
                return self.skeleton_plan(
                    expected,
                    Some(Skeleton {
                        nodes: pristine.nodes.clone(),
                        rig: Some(pristine.rig.clone()),
                        bases: pristine.bases.clone(),
                        renamed: BTreeSet::new(),
                        clips,
                        restructured: false,
                        rested: BTreeSet::new(),
                    }),
                );
            }
            JointEdit::Fit { targets } => {
                let mut moved = false;
                for (node, translation) in fit_order(&nodes, targets)? {
                    cancel.check()?;
                    let local = rest_parent_world(&nodes, node)
                        .ok_or_else(|| refused("Сустав без родителя не подгоняется"))?
                        .inverse()
                        .transform_point3(glam::Vec3::from_array(translation));
                    let changed = rest_joint(
                        &mut nodes,
                        &mut rig,
                        &mut bases,
                        &mut clips,
                        node,
                        local.to_array(),
                        false,
                        cancel,
                    )?;
                    moved |= !changed.is_empty();
                    rested.extend(changed);
                }
                if !moved {
                    return self.skeleton_plan(expected, None);
                }
            }
            JointEdit::Rest {
                node,
                translation,
                hold,
            } => {
                let changed = rest_joint(
                    &mut nodes,
                    &mut rig,
                    &mut bases,
                    &mut clips,
                    *node,
                    *translation,
                    *hold,
                    cancel,
                )?;
                if changed.is_empty() {
                    return self.skeleton_plan(expected, None);
                }
                rested.extend(changed);
            }
        }
        self.skeleton_plan(
            expected,
            Some(Skeleton {
                nodes,
                rig: Some(rig),
                bases,
                renamed,
                clips,
                restructured,
                rested,
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
        let retained = match &skeleton {
            Some(next) => next
                .bytes()
                .max(self.skeleton_bytes())
                .checked_add(crate::skin_animation::animation_history_bytes(
                    &self.animation_clips,
                    &next.clips,
                    &Cancellation::default(),
                )?)
                .ok_or_else(crate::indices::budget)?,
            None => 0,
        };
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
        std::mem::swap(&mut self.animation_clips, &mut skeleton.clips);
        std::mem::swap(&mut self.restructured, &mut skeleton.restructured);
        std::mem::swap(&mut self.rested, &mut skeleton.rested);
    }
    /// Nodes whose rest transform changed since opening.
    pub fn rested_nodes(&self) -> &BTreeSet<usize> {
        &self.rested
    }
    /// Joints were added or deleted since opening.
    pub fn is_restructured(&self) -> bool {
        self.restructured
    }
}

/// RIG-04: `node` and its subtree leave the scene (their nodes stay in the
/// list, unlinked). Refused for a node that is not a skin joint, a scene
/// root, a subtree with meshes, a subtree holding a skin's `skeleton` root,
/// a skin left without joints, weights with no remaining joint above to go
/// to, a primitive bound to two skins, or a clip left without channels.
fn delete_joint(
    nodes: &mut [Node],
    rig: &mut RigInspection,
    clips: &mut [Arc<AnimationClip>],
    node: NodeId,
    cancel: &Cancellation,
) -> AssetResult<()> {
    if node.0 >= nodes.len() {
        return Err(refused("Неизвестный сустав"));
    }
    if !rig.skins.iter().any(|skin| skin.joints.contains(&node)) {
        return Err(refused("Удалять можно только сустав скина."));
    }
    let mut parents = vec![None; nodes.len()];
    for (index, item) in nodes.iter().enumerate() {
        for child in &item.children {
            if let Some(slot) = parents.get_mut(child.0) {
                *slot = Some(index);
            }
        }
    }
    let Some(parent) = parents[node.0] else {
        return Err(refused("Корневой узел сцены не удаляется."));
    };
    let mut gone = vec![false; nodes.len()];
    let mut pending = vec![node.0];
    while let Some(n) = pending.pop() {
        cancel.check()?;
        if std::mem::replace(&mut gone[n], true) {
            continue;
        }
        if !nodes[n].meshes.is_empty() {
            return Err(refused(
                "Под суставом есть объект с мешем — сначала уберите его из-под сустава.",
            ));
        }
        pending.extend(nodes[n].children.iter().map(|c| c.0));
    }
    nodes[parent].children.retain(|child| *child != node);

    for s in 0..rig.skins.len() {
        cancel.check()?;
        let skin = &rig.skins[s];
        let removed: Vec<bool> = skin.joints.iter().map(|j| gone[j.0]).collect();
        if !removed.contains(&true) {
            continue;
        }
        if skin.skeleton.is_some_and(|root| gone[root.0]) {
            return Err(refused("Корень скелета (skeleton) не удаляется."));
        }
        if removed.iter().all(|r| *r) {
            return Err(refused("Нельзя удалить все суставы скина."));
        }
        // The nearest joint of this skin above the deleted one.
        let mut at = Some(parent);
        let mut target = None;
        let mut steps = 0;
        while let Some(p) = at {
            if let Some(slot) = skin.joints.iter().position(|j| j.0 == p) {
                target = Some(slot);
                break;
            }
            at = parents[p];
            steps += 1;
            if steps > nodes.len() {
                break;
            }
        }
        let mut map = vec![0u16; skin.joints.len()];
        let mut next = 0u16;
        for (slot, gone) in removed.iter().enumerate() {
            if !gone {
                map[slot] = next;
                next += 1;
            }
        }
        let target = target.map(|t| map[t]);
        let bound: BTreeSet<usize> = nodes
            .iter()
            .zip(&rig.node_skins)
            .filter(|(_, skin)| **skin == Some(s))
            .flat_map(|(n, _)| n.meshes.iter().map(|m| m.0))
            .collect();
        for &mesh in &bound {
            let shared = nodes.iter().zip(&rig.node_skins).any(|(n, other)| {
                other.is_some_and(|k| k != s) && n.meshes.iter().any(|m| m.0 == mesh)
            });
            if shared {
                return Err(refused(
                    "Примитив привязан к двум скинам — удаление не поддерживается.",
                ));
            }
            let Some(old) = rig.influences.get(mesh).cloned().flatten() else {
                continue;
            };
            let mut out = Vec::new();
            out.try_reserve_exact(old.len())
                .map_err(|_| crate::indices::budget())?;
            for (i, influence) in old.iter().enumerate() {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                let mut joints = [0u16; 4];
                let mut weights = [0f32; 4];
                let mut used = 0;
                for k in 0..4 {
                    let weight = influence.weights[k];
                    if weight <= 0. {
                        continue;
                    }
                    let slot = usize::from(influence.joints[k]);
                    let new = if removed[slot] {
                        target.ok_or_else(|| {
                            refused(
                                "Весам удаляемого сустава некому перейти: выше нет сустава скина.",
                            )
                        })?
                    } else {
                        map[slot]
                    };
                    match (0..used).find(|&j| joints[j] == new) {
                        Some(j) => weights[j] += weight,
                        None => {
                            joints[used] = new;
                            weights[used] = weight;
                            used += 1;
                        }
                    }
                }
                out.push(Influence { joints, weights });
            }
            rig.influences[mesh] = Some(Arc::from(out));
            rig.edited_influences.insert(mesh);
        }
        let skin = &mut rig.skins[s];
        let mut keep = removed.iter().map(|r| !r);
        skin.joints.retain(|_| keep.next().unwrap_or(true));
        let mut keep = removed.iter().map(|r| !r);
        skin.inverse_bind.retain(|_| keep.next().unwrap_or(true));
        let mut used = vec![false; skin.joints.len()];
        for &mesh in &bound {
            if let Some(weights) = &rig.influences[mesh] {
                for influence in weights.iter() {
                    for k in 0..4 {
                        if influence.weights[k] > 0. {
                            used[usize::from(influence.joints[k])] = true;
                        }
                    }
                }
            }
        }
        rig.reports[s].unused_joints = used
            .iter()
            .enumerate()
            .filter_map(|(i, used)| (!used).then_some(i))
            .collect();
    }
    for clip in clips.iter_mut() {
        cancel.check()?;
        if !clip.tracks().iter().any(|t| gone[t.node().0]) {
            continue;
        }
        let tracks: Vec<_> = clip
            .tracks()
            .iter()
            .filter(|t| !gone[t.node().0])
            .cloned()
            .collect();
        if tracks.is_empty() {
            return Err(refused(&format!(
                "Клип «{}» анимирует только удаляемые суставы и остался бы пустым.",
                clip.name
            )));
        }
        *clip = Arc::new(clip.with_tracks(tracks, cancel)?);
    }
    Ok(())
}

/// RIG-03: moves `node`'s rest translation and rebinds. For every skin
/// joint whose rest world changes (the node and everything under it):
/// `inverse bind' = inverse(W') × W × inverse bind`, so `W' × bind'` — the
/// skinning matrix at rest — stays the same and the mesh does not move.
/// The node's animation base and the bases of its tracks get the new
/// translation. With `hold`, each direct child takes the opposite move in
/// its own translation, so its rest world (and everything under it) stays.
/// Returns the nodes whose rest transform changed (empty: nothing to do).
#[allow(clippy::too_many_arguments)]
fn rest_joint(
    nodes: &mut [Node],
    rig: &mut RigInspection,
    bases: &mut [Option<Transform>],
    clips: &mut [Arc<AnimationClip>],
    node: NodeId,
    translation: [f32; 3],
    hold: bool,
    cancel: &Cancellation,
) -> AssetResult<Vec<usize>> {
    use glam::{Mat4, Vec3};
    if node.0 >= nodes.len() || !rig.skins.iter().any(|skin| skin.joints.contains(&node)) {
        return Err(refused("Положение меняется только у сустава скина."));
    }
    let Some(mut base) = bases.get(node.0).copied().flatten() else {
        return Err(refused(
            "У этого сустава матричная трансформация — его положение здесь не меняется.",
        ));
    };
    let offset = Vec3::from_array(translation);
    if !offset.is_finite() || offset.length() > 1e6 {
        return Err(refused(
            "Положение сустава должно быть конечным и не дальше 10⁶.",
        ));
    }
    if base.translation == translation {
        return Ok(Vec::new());
    }
    let children: Vec<usize> = if hold {
        nodes[node.0].children.iter().map(|c| c.0).collect()
    } else {
        Vec::new()
    };
    let mut held = Vec::new();
    for &child in &children {
        let Some(child_base) = bases.get(child).copied().flatten() else {
            return Err(refused(
                "У дочернего узла матричная трансформация — удержать его на месте нельзя; снимите «Дочерние остаются».",
            ));
        };
        held.push((child, child_base));
    }
    // Rest worlds of every node (authored locals, no placement: the binds
    // live in the same space).
    let mut parents = vec![None; nodes.len()];
    for (index, item) in nodes.iter().enumerate() {
        for child in &item.children {
            if let Some(slot) = parents.get_mut(child.0) {
                *slot = Some(index);
            }
        }
    }
    let world = |nodes: &[Node], n: usize| -> Mat4 {
        let mut chain = vec![n];
        while let Some(p) = parents[chain[chain.len() - 1]] {
            if chain.len() > nodes.len() {
                break;
            }
            chain.push(p);
        }
        chain.iter().rev().fold(Mat4::IDENTITY, |m, i| {
            m * Mat4::from_cols_array_2d(&nodes[*i].local)
        })
    };
    let mut subtree = vec![node.0];
    let mut seen = vec![false; nodes.len()];
    let mut at = 0;
    while at < subtree.len() {
        cancel.check()?;
        let n = subtree[at];
        at += 1;
        if std::mem::replace(&mut seen[n], true) {
            continue;
        }
        subtree.extend(nodes[n].children.iter().map(|c| c.0));
    }
    let before: Vec<(usize, Mat4)> = subtree.iter().map(|&n| (n, world(nodes, n))).collect();
    let old_local = Mat4::from_cols_array_2d(&nodes[node.0].local);
    base.translation = translation;
    let new_local = base.matrix();
    nodes[node.0].local = new_local.to_cols_array_2d();
    bases[node.0] = Some(base);
    let mut changed = vec![node.0];
    // Held children: child' = inverse(J') × J × child, which only moves the
    // translation (J keeps its rotation and scale).
    let keep = new_local.inverse() * old_local;
    for (child, mut child_base) in held {
        let moved = keep.transform_point3(Vec3::from_array(child_base.translation));
        if !moved.is_finite() {
            return Err(refused(
                "Удержание дочернего сустава дало неконечное положение",
            ));
        }
        if moved.to_array() == child_base.translation {
            continue;
        }
        child_base.translation = moved.to_array();
        nodes[child].local = child_base.matrix().to_cols_array_2d();
        bases[child] = Some(child_base);
        for clip in clips.iter_mut() {
            if clip.tracks().iter().any(|t| t.node().0 == child) {
                *clip = Arc::new(clip.rebased(NodeId(child), child_base));
            }
        }
        changed.push(child);
    }
    for skin in &mut rig.skins {
        for (slot, joint) in skin.joints.iter().enumerate() {
            cancel.check()?;
            let Some((_, old)) = before.iter().find(|(n, _)| *n == joint.0) else {
                continue;
            };
            let new = world(nodes, joint.0);
            let bind = new.inverse() * *old * Mat4::from_cols_array_2d(&skin.inverse_bind[slot]);
            if !bind.is_finite() || bind.determinant() == 0. {
                return Err(refused("Перепривязка дала вырожденную матрицу"));
            }
            skin.inverse_bind[slot] = bind.to_cols_array_2d();
        }
    }
    for clip in clips.iter_mut() {
        if clip.tracks().iter().any(|t| t.node() == node) {
            *clip = Arc::new(clip.rebased(node, base));
        }
    }
    Ok(changed)
}

/// Fit targets parents first (by depth), so a joint's target is reached
/// after its parent moved; unknown nodes and duplicates are refused.
fn fit_order(
    nodes: &[Node],
    targets: &[(NodeId, [f32; 3])],
) -> AssetResult<Vec<(NodeId, [f32; 3])>> {
    let mut parents = vec![None; nodes.len()];
    for (index, item) in nodes.iter().enumerate() {
        for child in &item.children {
            if let Some(slot) = parents.get_mut(child.0) {
                *slot = Some(index);
            }
        }
    }
    let depth = |mut n: usize| {
        let mut d = 0;
        while let Some(p) = parents[n] {
            n = p;
            d += 1;
            if d > nodes.len() {
                break;
            }
        }
        d
    };
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for &(node, target) in targets {
        if node.0 >= nodes.len() || !seen.insert(node.0) || !target.iter().all(|v| v.is_finite()) {
            return Err(refused("Неверный список подгонки суставов"));
        }
        ordered.push((depth(node.0), node, target));
    }
    ordered.sort_by_key(|(d, n, _)| (*d, n.0));
    Ok(ordered.into_iter().map(|(_, n, t)| (n, t)).collect())
}

/// Rest world of `node`'s parent (authored locals, no placement).
fn rest_parent_world(nodes: &[Node], node: NodeId) -> Option<glam::Mat4> {
    let parent = nodes.iter().position(|n| n.children.contains(&node))?;
    let mut chain = vec![parent];
    while let Some(p) = nodes
        .iter()
        .position(|n| n.children.contains(&NodeId(chain[chain.len() - 1])))
    {
        if chain.len() > nodes.len() {
            return None;
        }
        chain.push(p);
    }
    Some(chain.iter().rev().fold(glam::Mat4::IDENTITY, |m, i| {
        m * glam::Mat4::from_cols_array_2d(&nodes[*i].local)
    }))
}

/// RIG-06: the current clips made to fit the file's skeleton again:
/// channels on added joints go, channels of deleted joints come back from
/// the file's clip with the same identity, rebased channels get the file's
/// bases. A clip left without channels is refused (it animates only added
/// joints).
fn reset_clips(
    document: &AssetDocument,
    pristine: &Pristine,
    clips: &mut [Arc<AnimationClip>],
    cancel: &Cancellation,
) -> AssetResult<()> {
    let source = document.source_node_count;
    let current: BTreeSet<usize> = document
        .rig
        .iter()
        .flat_map(|rig| &rig.skins)
        .flat_map(|skin| skin.joints.iter().map(|j| j.0))
        .collect();
    let deleted: BTreeSet<usize> = pristine
        .rig
        .skins
        .iter()
        .flat_map(|skin| skin.joints.iter().map(|j| j.0))
        .filter(|j| *j < source && !current.contains(j))
        .collect();
    for clip in clips.iter_mut() {
        cancel.check()?;
        let mut tracks: Vec<_> = clip
            .tracks()
            .iter()
            .filter(|t| t.node().0 < source)
            .cloned()
            .collect();
        if let Some(original) = pristine.clips.iter().find(|c| c.id() == clip.id()) {
            for track in original.tracks() {
                let missing = !tracks
                    .iter()
                    .any(|t| t.node() == track.node() && t.kind() == track.kind());
                if deleted.contains(&track.node().0) && missing {
                    tracks.push(track.clone());
                }
            }
        }
        let same = tracks.len() == clip.tracks().len()
            && tracks
                .iter()
                .zip(clip.tracks())
                .all(|(a, b)| Arc::ptr_eq(a, b));
        if !same {
            if tracks.is_empty() {
                return Err(refused(&format!(
                    "Клип «{}» анимирует только добавленные суставы — удалите его перед сбросом.",
                    clip.name
                )));
            }
            *clip = Arc::new(clip.with_tracks(tracks, cancel)?);
        }
        for &node in &document.rested {
            let node = NodeId(node);
            if let Some(base) = pristine.bases.get(node.0).copied().flatten()
                && clip.tracks().iter().any(|t| t.node() == node)
            {
                *clip = Arc::new(clip.rebased(node, base));
            }
        }
    }
    Ok(())
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
    if skeleton
        .clips
        .iter()
        .flat_map(|clip| clip.tracks())
        .any(|track| track.node().0 >= nodes.len())
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
