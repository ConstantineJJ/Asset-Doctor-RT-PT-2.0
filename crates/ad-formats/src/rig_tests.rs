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
