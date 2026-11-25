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
fn a_channel_with_a_missing_sampler_is_refused() {
    let error = open(BAD_SAMPLER).err().expect("refused");
    assert!(error.message.contains("sampler"), "{}", error.message);
}
