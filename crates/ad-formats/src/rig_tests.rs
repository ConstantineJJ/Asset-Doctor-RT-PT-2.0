//! P7 rig and animation import with small authored fixtures:
//! - Skin_Checker: one skin, rest pose only;
//! - Animation_Checker: a three-joint skin (Root → Tip, Unused) with clips
//!   «Linear» and «Step and cubic»;
//! - Animation_Bad_Sampler: a channel pointing at a missing sampler.
use super::*;
use glam::Mat4;

fn open(bytes: &[u8]) -> AssetResult<AssetDocument> {
    import_glb(
        Arc::from(bytes),
        "rig".into(),
        DocumentId(7),
        ImportLimits::default(),
        &Cancellation::default(),
    )
}
const SKIN: &[u8] = include_bytes!("../../../tests/fixtures/Skin_Checker.glb");
const ANIMATED: &[u8] = include_bytes!("../../../tests/fixtures/Animation_Checker.glb");
const BAD_SAMPLER: &[u8] = include_bytes!("../../../tests/fixtures/Animation_Bad_Sampler.glb");

fn matrix(m: [[f32; 4]; 4]) -> Mat4 {
    Mat4::from_cols_array_2d(&m)
}

#[test]
fn a_skinned_model_opens_with_its_joints_and_a_rest_pose() {
    let doc = open(SKIN).unwrap();
    let rig = doc.rig().expect("skin");
    assert_eq!(rig.skins.len(), 1);
    assert!(!rig.skins[0].joints.is_empty());
    assert_eq!(rig.skins[0].joints.len(), rig.skins[0].inverse_bind.len());
    let snapshot = doc
        .snapshot(ImportLimits::default(), &Cancellation::default())
        .unwrap();
    assert!(!snapshot.indices.is_empty());
    assert!(doc.is_read_only(), "geometry edits stay refused for skins");
    // Every joint has a world matrix in the authored pose.
    let worlds = doc
        .view()
        .node_worlds(None, &Cancellation::default())
        .unwrap();
    for joint in &rig.skins[0].joints {
        assert!(
            worlds[joint.0].is_some_and(|m| matrix(m).is_finite()),
            "{joint:?}"
        );
    }
}

#[test]
fn animated_joint_worlds_follow_the_clip_and_compose_with_parents() {
    let doc = open(ANIMATED).unwrap();
    let clips = doc.animation_clips();
    let names: Vec<_> = clips.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Linear", "Step and cubic"]);
    let cancel = Cancellation::default();
    let view = doc.view();
    let rest = view.node_worlds(None, &cancel).unwrap();
    let (root, tip) = (NodeId(0), NodeId(1));
    assert_eq!(doc.nodes()[root.0].name, "Root");
    assert_eq!(doc.nodes()[tip.0].name, "Tip");
    let tip_local = matrix(rest[tip.0].unwrap()) * Mat4::IDENTITY;
    let tip_local = matrix(rest[root.0].unwrap()).inverse() * tip_local;
    let mut moved = false;
    for (clip, active) in clips.iter().enumerate() {
        let duration = active.duration;
        assert!(duration > 0.);
        for step in 0..=8 {
            let time = duration * step as f32 / 8.;
            let posed = view.node_worlds(Some((clip, time, None)), &cancel).unwrap();
            let (r, t) = (
                matrix(posed[root.0].unwrap()),
                matrix(posed[tip.0].unwrap()),
            );
            assert!(r.is_finite() && t.is_finite());
            moved |= !r.abs_diff_eq(matrix(rest[root.0].unwrap()), 1e-5)
                || !t.abs_diff_eq(matrix(rest[tip.0].unwrap()), 1e-5);
            // Tip keeps its parent relation unless the clip animates Tip.
            let animated_tip = active.tracks().iter().any(|track| track.node() == tip);
            if !animated_tip {
                assert!(t.abs_diff_eq(r * tip_local, 1e-4), "clip {clip} t={time}");
            }
            // The posed geometry is built from the same pose.
            let snapshot = view
                .animated_snapshot(clip, time, None, ImportLimits::default(), &cancel)
                .unwrap();
            assert!(snapshot.positions.iter().flatten().all(|v| v.is_finite()));
        }
    }
    assert!(moved, "the clips move the skeleton");
    // Out-of-range clip and non-finite time are refused.
    assert!(view.node_worlds(Some((99, 0., None)), &cancel).is_err());
    assert!(
        view.node_worlds(Some((0, f32::NAN, None)), &cancel)
            .is_err()
    );
}

#[test]
fn a_node_path_matches_the_scene_worlds_at_every_sample() {
    let doc = open(ANIMATED).unwrap();
    let cancel = Cancellation::default();
    let view = doc.view();
    for (clip, active) in doc.animation_clips().iter().enumerate() {
        for node in [NodeId(0), NodeId(1)] {
            let path = view.node_path(node, clip, 9, &cancel).unwrap();
            assert_eq!(path.len(), 9);
            assert_eq!(path[0].0, 0.);
            assert_eq!(path[8].0, active.duration);
            for (time, world) in &path {
                let all = view
                    .node_worlds(Some((clip, *time, None)), &cancel)
                    .unwrap();
                assert!(
                    matrix(*world).abs_diff_eq(matrix(all[node.0].unwrap()), 1e-5),
                    "clip {clip} node {node:?} t={time}"
                );
            }
        }
    }
    // At least two samples; unknown clip or node refused.
    assert_eq!(view.node_path(NodeId(0), 0, 0, &cancel).unwrap().len(), 2);
    assert!(view.node_path(NodeId(0), 99, 9, &cancel).is_err());
    assert!(view.node_path(NodeId(999), 0, 9, &cancel).is_err());
}

#[test]
fn a_channel_with_a_missing_sampler_is_refused() {
    let error = open(BAD_SAMPLER).err().expect("refused");
    assert!(error.message.contains("sampler"), "{}", error.message);
}

#[test]
fn renamed_and_added_joints_are_saved_read_back_and_undone() {
    use ad_document::JointEdit;
    let cancel = Cancellation::default();
    let mut doc = open(ANIMATED).unwrap();
    let source = doc.nodes().len();
    let tip = NodeId(1);
    let rig = doc.rig().unwrap();
    let slot = rig.skins[0].joints.iter().position(|j| *j == tip).unwrap();
    let tip_bind = matrix(rig.skins[0].inverse_bind[slot]);
    let joints_before = rig.skins[0].joints.len();
    let apply = |doc: &mut AssetDocument, edit: JointEdit| {
        let plan = doc.prepare_joint_edit(doc.tag(), &edit, &cancel).unwrap();
        {
            // The candidate saves and reads back before it is applied.
            let candidate = plan.candidate(doc, &cancel).unwrap();
            let view = doc.candidate_view(candidate, &cancel).unwrap();
            assert!(view.skeleton_changed());
            verify_animation_export(
                &view,
                ExportLimits::default(),
                ImportLimits::default(),
                &cancel,
            )
            .unwrap();
            view.snapshot(ImportLimits::default(), &cancel).unwrap();
        }
        doc.apply_triangle_removal(plan, ImportLimits::default(), &cancel)
            .unwrap()
    };
    let renamed = apply(
        &mut doc,
        JointEdit::Rename {
            node: tip,
            name: "  Кончик  ".into(),
        },
    );
    assert_eq!(doc.nodes()[tip.0].name, "Кончик");
    let added = apply(
        &mut doc,
        JointEdit::Add {
            parent: tip,
            name: "Tip_end".into(),
            translation: [0., 0.5, 0.],
        },
    );
    // The new joint: last node, child of Tip, in the skin, bind continued
    // from Tip's, no weights.
    assert_eq!(doc.nodes().len(), source + 1);
    let new = NodeId(source);
    assert_eq!(doc.nodes()[new.0].name, "Tip_end");
    assert_eq!(doc.nodes()[tip.0].children.last(), Some(&new));
    let skin = &doc.rig().unwrap().skins[0];
    assert_eq!(skin.joints.len(), joints_before + 1);
    assert_eq!(skin.joints.last(), Some(&new));
    let expected = Mat4::from_translation(glam::Vec3::new(0., 0.5, 0.)).inverse() * tip_bind;
    assert!(matrix(*skin.inverse_bind.last().unwrap()).abs_diff_eq(expected, 1e-6));
    assert_eq!(
        doc.rig().unwrap().reports[0].unused_joints.last(),
        Some(&joints_before)
    );
    let worlds = doc.view().node_worlds(None, &cancel).unwrap();
    let offset = matrix(worlds[tip.0].unwrap()).inverse() * matrix(worlds[new.0].unwrap());
    assert!(offset.abs_diff_eq(Mat4::from_translation(glam::Vec3::new(0., 0.5, 0.)), 1e-5));
    // Saved and opened again: same names, nodes and skin; clips untouched.
    let saved = crate::prepare_glb(
        &doc,
        doc.tag(),
        ExportLimits::default(),
        ImportLimits::default(),
        &cancel,
    )
    .unwrap();
    let back = open(saved.bytes()).unwrap();
    assert_eq!(back.nodes().len(), source + 1);
    assert_eq!(back.nodes()[tip.0].name, "Кончик");
    assert_eq!(back.nodes()[new.0].name, "Tip_end");
    assert_eq!(back.rig().unwrap().skins[0].joints, skin.joints);
    assert_eq!(back.animation_clips().len(), doc.animation_clips().len());
    assert!(back.can_animate_node(new), "the new joint can be keyed");
    // Undo both: the file's skeleton again.
    for applied in [added, renamed] {
        let mut change = applied.change.unwrap();
        doc.exchange_indices(doc.tag(), &mut change, ImportLimits::default(), &cancel)
            .unwrap();
    }
    assert_eq!(doc.nodes().len(), source);
    assert_eq!(doc.nodes()[tip.0].name, "Tip");
    assert_eq!(doc.rig().unwrap().skins[0].joints.len(), joints_before);
    // Refusals: empty name, unknown node, non-joint parent, zero offset.
    let tag = doc.tag();
    for edit in [
        JointEdit::Rename {
            node: tip,
            name: "   ".into(),
        },
        JointEdit::Rename {
            node: NodeId(999),
            name: "x".into(),
        },
        JointEdit::Add {
            parent: tip,
            name: "x".into(),
            translation: [0.; 3],
        },
        JointEdit::Add {
            parent: tip,
            name: "x".into(),
            translation: [f32::NAN, 0., 0.],
        },
    ] {
        assert!(
            doc.prepare_joint_edit(tag, &edit, &cancel).is_err(),
            "{edit:?}"
        );
    }
    let mesh_node = (0..doc.nodes().len())
        .map(NodeId)
        .find(|n| !doc.nodes()[n.0].meshes.is_empty())
        .unwrap();
    assert!(
        doc.prepare_joint_edit(
            tag,
            &JointEdit::Add {
                parent: mesh_node,
                name: "x".into(),
                translation: [0., 1., 0.],
            },
            &cancel
        )
        .is_err(),
        "only under a skin joint"
    );
    // A rename to the same name changes nothing.
    let same = doc
        .prepare_joint_edit(
            tag,
            &JointEdit::Rename {
                node: tip,
                name: "Tip".into(),
            },
            &cancel,
        )
        .unwrap();
    assert!(!same.changes());
}

#[test]
fn deleting_a_joint_moves_its_weights_up_saves_and_undoes() {
    use ad_document::{ClipEdit, JointEdit, MeshId};
    let cancel = Cancellation::default();
    let mut doc = open(ANIMATED).unwrap();
    let (root, tip, unused) = (NodeId(0), NodeId(1), NodeId(2));
    let apply = |doc: &mut AssetDocument, plan: ad_document::PreparedRemoval| {
        {
            let candidate = plan.candidate(doc, &cancel).unwrap();
            let view = doc.candidate_view(candidate, &cancel).unwrap();
            verify_animation_export(
                &view,
                ExportLimits::default(),
                ImportLimits::default(),
                &cancel,
            )
            .unwrap();
            view.snapshot(ImportLimits::default(), &cancel).unwrap();
        }
        doc.apply_triangle_removal(plan, ImportLimits::default(), &cancel)
            .unwrap()
    };
    let delete = |doc: &AssetDocument, node| {
        doc.prepare_joint_edit(doc.tag(), &JointEdit::Delete { node }, &cancel)
    };
    // Refused: Tip is all of clip «Linear»; Root is the skeleton root; a
    // mesh node is not a joint.
    let error = delete(&doc, tip).err().expect("clip would be empty");
    assert!(error.message.contains("Linear"), "{}", error.message);
    assert!(delete(&doc, root).is_err());
    assert!(delete(&doc, NodeId(3)).is_err());
    let before: Vec<Vec<ad_document::Influence>> = (0..doc.primitives().len())
        .filter_map(|m| doc.rig().unwrap().primitive_influences(MeshId(m)))
        .map(<[_]>::to_vec)
        .collect();
    let tip_weight: f32 = before
        .iter()
        .flatten()
        .flat_map(|i| i.joints.iter().zip(i.weights))
        .filter(|(j, _)| **j == 1)
        .map(|(_, w)| w)
        .sum();
    assert!(tip_weight > 0., "the fixture weights Tip");
    // Unused: no weights, no channels.
    let plan = delete(&doc, unused).unwrap();
    let removed_unused = apply(&mut doc, plan);
    assert_eq!(doc.rig().unwrap().skins[0].joints, vec![root, tip]);
    assert!(!doc.nodes()[root.0].children.contains(&unused));
    // Tip, after its clip is gone: its weights go to Root (slot 0).
    let linear = doc
        .prepare_clip_edit(doc.tag(), &ClipEdit::Delete { clip: 0 }, &cancel)
        .unwrap();
    let removed_clip = apply(&mut doc, linear);
    let plan = delete(&doc, tip).unwrap();
    let removed_tip = apply(&mut doc, plan);
    let rig = doc.rig().unwrap();
    assert_eq!(rig.skins[0].joints, vec![root]);
    assert_eq!(rig.skins[0].inverse_bind.len(), 1);
    let after = rig.primitive_influences(MeshId(0)).unwrap();
    for (old, new) in before[0].iter().zip(after) {
        assert!(
            new.joints
                .iter()
                .zip(new.weights)
                .all(|(j, w)| w == 0. || *j == 0)
        );
        let (a, b): (f32, f32) = (old.weights.iter().sum(), new.weights.iter().sum());
        assert!((a - b).abs() < 1e-6, "weight kept: {a} vs {b}");
    }
    assert!(
        doc.animation_clips()
            .iter()
            .all(|clip| clip.tracks().iter().all(|t| t.node() != tip))
    );
    // Saved and opened again: the skin, weights and clips match.
    let saved = crate::prepare_glb(
        &doc,
        doc.tag(),
        ExportLimits::default(),
        ImportLimits::default(),
        &cancel,
    )
    .unwrap();
    let back = open(saved.bytes()).unwrap();
    assert_eq!(back.rig().unwrap().skins[0].joints, vec![root]);
    assert!(back.nodes()[root.0].children.is_empty());
    assert_eq!(back.animation_clips().len(), 1);
    assert!(
        back.rig()
            .unwrap()
            .matches(doc.rig().unwrap(), &cancel)
            .unwrap()
    );
    // Undo all three: the file's skeleton, weights and clips again.
    for applied in [removed_tip, removed_clip, removed_unused] {
        let mut change = applied.change.unwrap();
        doc.exchange_indices(doc.tag(), &mut change, ImportLimits::default(), &cancel)
            .unwrap();
    }
    assert_eq!(doc.rig().unwrap().skins[0].joints, vec![root, tip, unused]);
    assert_eq!(doc.animation_clips().len(), 2);
    assert_eq!(
        doc.rig().unwrap().primitive_influences(MeshId(0)).unwrap(),
        before[0].as_slice()
    );
}

#[test]
fn a_longer_bone_moves_the_joint_but_not_the_mesh() {
    use ad_document::JointEdit;
    let cancel = Cancellation::default();
    let mut doc = open(ANIMATED).unwrap();
    let tip = NodeId(1);
    let rest = |doc: &AssetDocument| {
        doc.snapshot(ImportLimits::default(), &cancel)
            .unwrap()
            .positions
    };
    let mesh_before = rest(&doc);
    let worlds = doc.view().node_worlds(None, &cancel).unwrap();
    let tip_before = matrix(worlds[tip.0].unwrap());
    let base = doc.rest_transform(tip).expect("TRS joint");
    let longer = base.translation.map(|v| v * 2.);
    let edit = JointEdit::Rest {
        node: tip,
        translation: longer,
        hold: false,
    };
    let plan = doc.prepare_joint_edit(doc.tag(), &edit, &cancel).unwrap();
    {
        let candidate = plan.candidate(&doc, &cancel).unwrap();
        let view = doc.candidate_view(candidate, &cancel).unwrap();
        verify_animation_export(
            &view,
            ExportLimits::default(),
            ImportLimits::default(),
            &cancel,
        )
        .unwrap();
    }
    let applied = doc
        .apply_triangle_removal(plan, ImportLimits::default(), &cancel)
        .unwrap();
    // The joint moved by its old offset once more (in the parent's space);
    // the mesh at rest stayed where it was.
    let worlds = doc.view().node_worlds(None, &cancel).unwrap();
    let tip_after = matrix(worlds[tip.0].unwrap());
    let parent = matrix(worlds[0].unwrap());
    let moved = tip_after.w_axis.truncate() - tip_before.w_axis.truncate();
    let expected = parent.transform_vector3(glam::Vec3::from_array(base.translation));
    assert!(moved.abs_diff_eq(expected, 1e-5), "{moved} vs {expected}");
    let mesh_after = rest(&doc);
    assert_eq!(mesh_before.len(), mesh_after.len());
    for (a, b) in mesh_before.iter().zip(&mesh_after) {
        assert!(
            glam::Vec3::from_array(*a).abs_diff_eq(glam::Vec3::from_array(*b), 1e-4),
            "{a:?} vs {b:?}"
        );
    }
    assert_eq!(doc.rest_transform(tip).unwrap().translation, longer);
    // Saved and opened again: the new rest and binds come back.
    let saved = crate::prepare_glb(
        &doc,
        doc.tag(),
        ExportLimits::default(),
        ImportLimits::default(),
        &cancel,
    )
    .unwrap();
    let back = open(saved.bytes()).unwrap();
    assert_eq!(back.rest_transform(tip).unwrap().translation, longer);
    assert!(
        back.rig()
            .unwrap()
            .matches(doc.rig().unwrap(), &cancel)
            .unwrap()
    );
    // Undo: the file's rest again; the same position is a no-op; a mesh
    // node or a NaN position is refused.
    let mut change = applied.change.unwrap();
    doc.exchange_indices(doc.tag(), &mut change, ImportLimits::default(), &cancel)
        .unwrap();
    assert_eq!(
        doc.rest_transform(tip).unwrap().translation,
        base.translation
    );
    let same = JointEdit::Rest {
        node: tip,
        translation: base.translation,
        hold: false,
    };
    assert!(
        !doc.prepare_joint_edit(doc.tag(), &same, &cancel)
            .unwrap()
            .changes()
    );
    for edit in [
        JointEdit::Rest {
            node: NodeId(3),
            translation: [0., 1., 0.],
            hold: false,
        },
        JointEdit::Rest {
            node: tip,
            translation: [f32::NAN, 0., 0.],
            hold: false,
        },
    ] {
        assert!(
            doc.prepare_joint_edit(doc.tag(), &edit, &cancel).is_err(),
            "{edit:?}"
        );
    }
}

#[test]
fn resetting_the_rig_brings_back_the_files_skeleton_and_fits_the_clips() {
    use ad_document::{ClipEdit, JointEdit, PreparedRemoval};
    let cancel = Cancellation::default();
    let file = open(ANIMATED).unwrap();
    let mut doc = open(ANIMATED).unwrap();
    let (tip, unused) = (NodeId(1), NodeId(2));
    let apply = |doc: &mut AssetDocument, plan: PreparedRemoval| {
        {
            let candidate = plan.candidate(doc, &cancel).unwrap();
            let view = doc.candidate_view(candidate, &cancel).unwrap();
            verify_animation_export(
                &view,
                ExportLimits::default(),
                ImportLimits::default(),
                &cancel,
            )
            .unwrap();
        }
        doc.apply_triangle_removal(plan, ImportLimits::default(), &cancel)
            .unwrap()
    };
    let joint = |doc: &mut AssetDocument, edit: JointEdit| {
        let plan = doc.prepare_joint_edit(doc.tag(), &edit, &cancel).unwrap();
        apply(doc, plan)
    };
    // Nothing changed yet: a reset is a no-op.
    assert!(
        !doc.prepare_joint_edit(doc.tag(), &JointEdit::Reset, &cancel)
            .unwrap()
            .changes()
    );
    let longer = doc
        .rest_transform(tip)
        .unwrap()
        .translation
        .map(|v| v * 1.5);
    for edit in [
        JointEdit::Rename {
            node: tip,
            name: "Кончик".into(),
        },
        JointEdit::Add {
            parent: tip,
            name: "Tip_end".into(),
            translation: [0., 0.3, 0.],
        },
        JointEdit::Rest {
            node: tip,
            translation: longer,
            hold: false,
        },
        JointEdit::Delete { node: unused },
    ] {
        joint(&mut doc, edit);
    }
    let reset = joint(&mut doc, JointEdit::Reset);
    // The file's skeleton, rig and poses again.
    assert_eq!(doc.nodes().len(), file.nodes().len());
    assert_eq!(doc.nodes()[tip.0].name, "Tip");
    assert!(
        doc.rig()
            .unwrap()
            .matches(file.rig().unwrap(), &cancel)
            .unwrap()
    );
    assert!(doc.renamed_nodes().is_empty() && doc.rested_nodes().is_empty());
    for clip in 0..file.animation_clips().len() {
        let d = file.animation_clips()[clip].duration;
        for time in [0., d * 0.5, d] {
            let a = doc
                .view()
                .node_worlds(Some((clip, time, None)), &cancel)
                .unwrap();
            let b = file
                .view()
                .node_worlds(Some((clip, time, None)), &cancel)
                .unwrap();
            for (a, b) in a.iter().zip(&b) {
                if let (Some(a), Some(b)) = (a, b) {
                    assert!(
                        matrix(*a).abs_diff_eq(matrix(*b), 1e-5),
                        "clip {clip} t={time}"
                    );
                }
            }
        }
    }
    let saved = crate::prepare_glb(
        &doc,
        doc.tag(),
        ExportLimits::default(),
        ImportLimits::default(),
        &cancel,
    )
    .unwrap();
    let back = open(saved.bytes()).unwrap();
    assert_eq!(back.nodes().len(), file.nodes().len());
    assert!(
        back.rig()
            .unwrap()
            .matches(file.rig().unwrap(), &cancel)
            .unwrap()
    );
    // Undo of the reset: the edited skeleton again.
    let mut change = reset.change.unwrap();
    doc.exchange_indices(doc.tag(), &mut change, ImportLimits::default(), &cancel)
        .unwrap();
    assert_eq!(doc.nodes().len(), file.nodes().len() + 1);
    assert_eq!(doc.nodes()[tip.0].name, "Кончик");
    // Deleted Tip: after a reset its channels are back in the clip it had.
    let mut doc = open(ANIMATED).unwrap();
    let plan = doc
        .prepare_clip_edit(doc.tag(), &ClipEdit::Delete { clip: 0 }, &cancel)
        .unwrap();
    apply(&mut doc, plan);
    joint(&mut doc, JointEdit::Delete { node: tip });
    assert!(
        doc.animation_clips()[0]
            .track(tip, ad_document::TrackKind::Translation)
            .is_none()
    );
    joint(&mut doc, JointEdit::Reset);
    assert!(
        doc.animation_clips()[0]
            .track(tip, ad_document::TrackKind::Translation)
            .is_some()
    );
    assert!(
        doc.rig()
            .unwrap()
            .matches(file.rig().unwrap(), &cancel)
            .unwrap()
    );
}

#[test]
fn joints_fit_to_the_bend_ring_of_the_mesh() {
    use ad_document::JointEdit;
    let cancel = Cancellation::default();
    let mut doc = open(ANIMATED).unwrap();
    let fits = doc.suggest_joint_fits(&cancel).unwrap();
    let tip = fits
        .iter()
        .find(|f| f.node == NodeId(1))
        .expect("Tip bends against Root in the fixture")
        .clone();
    assert!(tip.samples > 0 && tip.bone > 0.);
    assert!(tip.suggested.iter().all(|v| v.is_finite()));
    let mesh = |doc: &AssetDocument| {
        doc.snapshot(ImportLimits::default(), &cancel)
            .unwrap()
            .positions
    };
    let before = mesh(&doc);
    let edit = JointEdit::Fit {
        targets: vec![(tip.node, tip.suggested)],
    };
    let plan = doc.prepare_joint_edit(doc.tag(), &edit, &cancel).unwrap();
    {
        let candidate = plan.candidate(&doc, &cancel).unwrap();
        let view = doc.candidate_view(candidate, &cancel).unwrap();
        verify_animation_export(
            &view,
            ExportLimits::default(),
            ImportLimits::default(),
            &cancel,
        )
        .unwrap();
    }
    doc.apply_triangle_removal(plan, ImportLimits::default(), &cancel)
        .unwrap();
    // The joint is where it was suggested; the mesh at rest did not move.
    let worlds = doc.view().node_worlds(None, &cancel).unwrap();
    let at = matrix(worlds[1].unwrap()).w_axis.truncate();
    assert!(
        at.abs_diff_eq(glam::Vec3::from_array(tip.suggested), 1e-4),
        "{at}"
    );
    for (a, b) in before.iter().zip(&mesh(&doc)) {
        assert!(glam::Vec3::from_array(*a).abs_diff_eq(glam::Vec3::from_array(*b), 1e-4));
    }
    // Fitting again to the same place changes nothing; a bad list is refused.
    let again = doc.prepare_joint_edit(doc.tag(), &edit, &cancel).unwrap();
    assert!(!again.changes());
    let twice = JointEdit::Fit {
        targets: vec![(tip.node, tip.suggested), (tip.node, tip.suggested)],
    };
    assert!(doc.prepare_joint_edit(doc.tag(), &twice, &cancel).is_err());
}

#[test]
fn a_joint_moves_alone_when_its_children_hold_their_place() {
    use ad_document::JointEdit;
    let cancel = Cancellation::default();
    let mut doc = open(ANIMATED).unwrap();
    let tip = NodeId(1);
    apply_joint(
        &mut doc,
        JointEdit::Add {
            parent: tip,
            name: "Tip_end".into(),
            translation: [0., 0.3, 0.],
        },
    );
    let end = NodeId(doc.nodes().len() - 1);
    let world_of = |doc: &AssetDocument, n: NodeId| {
        let worlds = doc.view().node_worlds(None, &cancel).unwrap();
        matrix(worlds[n.0].unwrap()).w_axis.truncate()
    };
    let mesh = |doc: &AssetDocument| {
        doc.snapshot(ImportLimits::default(), &cancel)
            .unwrap()
            .positions
    };
    let (end_before, mesh_before) = (world_of(&doc, end), mesh(&doc));
    let base = doc.rest_transform(tip).unwrap();
    let longer = base.translation.map(|v| v * 1.5);
    // Held: the tip moves, its child stays where it was, the mesh too.
    let edit = JointEdit::Rest {
        node: tip,
        translation: longer,
        hold: true,
    };
    let plan = doc.prepare_joint_edit(doc.tag(), &edit, &cancel).unwrap();
    {
        let candidate = plan.candidate(&doc, &cancel).unwrap();
        let view = doc.candidate_view(candidate, &cancel).unwrap();
        verify_animation_export(
            &view,
            ExportLimits::default(),
            ImportLimits::default(),
            &cancel,
        )
        .unwrap();
    }
    let applied = doc
        .apply_triangle_removal(plan, ImportLimits::default(), &cancel)
        .unwrap();
    assert!(
        world_of(&doc, end).abs_diff_eq(end_before, 1e-5),
        "{} vs {end_before}",
        world_of(&doc, end)
    );
    for (a, b) in mesh_before.iter().zip(&mesh(&doc)) {
        assert!(glam::Vec3::from_array(*a).abs_diff_eq(glam::Vec3::from_array(*b), 1e-4));
    }
    // Saved and opened again: the held child is still in place.
    let saved = crate::prepare_glb(
        &doc,
        doc.tag(),
        ExportLimits::default(),
        ImportLimits::default(),
        &cancel,
    )
    .unwrap();
    let back = open(saved.bytes()).unwrap();
    assert!(world_of(&back, end).abs_diff_eq(end_before, 1e-5));
    // Undo, then the same move with following children: the child moves
    // with the tip.
    let mut change = applied.change.unwrap();
    doc.exchange_indices(doc.tag(), &mut change, ImportLimits::default(), &cancel)
        .unwrap();
    assert!(world_of(&doc, end).abs_diff_eq(end_before, 1e-5));
    let tip_before = world_of(&doc, tip);
    apply_joint(
        &mut doc,
        JointEdit::Rest {
            node: tip,
            translation: longer,
            hold: false,
        },
    );
    let shift = world_of(&doc, tip) - tip_before;
    assert!(shift.length() > 1e-3);
    assert!(
        (world_of(&doc, end) - end_before).abs_diff_eq(shift, 1e-5),
        "the child follows"
    );
}

fn apply_joint(doc: &mut AssetDocument, edit: ad_document::JointEdit) {
    let cancel = Cancellation::default();
    let plan = doc.prepare_joint_edit(doc.tag(), &edit, &cancel).unwrap();
    doc.apply_triangle_removal(plan, ImportLimits::default(), &cancel)
        .unwrap();
}

#[test]
fn the_gpu_skinning_layout_matches_the_cpu_pose() {
    let cancel = Cancellation::default();
    for source in [SKIN, ANIMATED] {
        let doc = open(source).unwrap();
        let view = doc.view();
        let layout = view
            .skin_layout(ImportLimits::default(), &cancel)
            .unwrap()
            .expect("a skinned or animated scene");
        let rest = doc.snapshot(ImportLimits::default(), &cancel).unwrap();
        assert_eq!(layout.vertices.len(), rest.positions.len());
        let mut poses: Vec<Option<(usize, f32)>> = vec![None];
        for (clip, active) in doc.animation_clips().iter().enumerate() {
            for step in 0..=4 {
                poses.push(Some((clip, active.duration * step as f32 / 4.)));
            }
        }
        for pose in poses {
            let expected = match pose {
                Some((clip, time)) => view
                    .animated_snapshot(clip, time, None, ImportLimits::default(), &cancel)
                    .unwrap(),
                None => doc.snapshot(ImportLimits::default(), &cancel).unwrap(),
            };
            let matrices = view
                .pose_matrices(&layout, pose.map(|(c, t)| (c, t, None)), &cancel)
                .unwrap();
            assert_eq!(matrices.len(), layout.matrices);
            for (i, vertex) in layout.vertices.iter().enumerate() {
                let (p, n, t) = ad_document::skin_reference(vertex, &matrices);
                let close = |a: &[f32], b: &[f32], eps: f32| {
                    a.iter()
                        .zip(b)
                        .all(|(a, b)| (a - b).abs() <= eps * (1. + b.abs()))
                };
                assert!(
                    close(&p, &expected.positions[i], 1e-5),
                    "{pose:?} v{i} {p:?} {:?}",
                    expected.positions[i]
                );
                assert!(
                    close(&n, &expected.normals[i], 1e-4),
                    "{pose:?} v{i} {n:?} {:?}",
                    expected.normals[i]
                );
                assert!(
                    close(&t, &expected.tangents[i], 1e-4),
                    "{pose:?} v{i} {t:?} {:?}",
                    expected.tangents[i]
                );
            }
        }
    }
    // Morph targets keep the CPU pose.
    let morph = open(include_bytes!("../../../tests/fixtures/Morph_Checker.glb")).unwrap();
    assert!(
        morph
            .view()
            .skin_layout(ImportLimits::default(), &cancel)
            .unwrap()
            .is_none()
    );
}
