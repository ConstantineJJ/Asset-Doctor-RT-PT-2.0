//! ADR-0065: a triangle whose third vertex a morph target lifts by 1 along
//! Y, and a clip that animates the weight from 0 to 1 in one second.
use super::*;
use ad_document::{ClipEdit, JointEdit};
use serde_json::json;

fn glb() -> Arc<[u8]> {
    let json = json!({
        "asset": {"version": "2.0"},
        "buffers": [{"byteLength": 96}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 6},
            {"buffer": 0, "byteOffset": 44, "byteLength": 36},
            {"buffer": 0, "byteOffset": 80, "byteLength": 8},
            {"buffer": 0, "byteOffset": 88, "byteLength": 8}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0,0,0], "max": [1,1,0]},
            {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"},
            {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0,0,0], "max": [0,1,0]},
            {"bufferView": 3, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1]},
            {"bufferView": 4, "componentType": 5126, "count": 2, "type": "SCALAR"}
        ],
        "meshes": [{
            "primitives": [{"attributes": {"POSITION": 0}, "indices": 1, "targets": [{"POSITION": 2}]}],
            "weights": [0.0],
            "extras": {"targetNames": ["Lift"]}
        }],
        "nodes": [{"mesh": 0, "name": "Face"}],
        "scenes": [{"nodes": [0]}],
        "scene": 0,
        "animations": [{
            "name": "Smile",
            "channels": [{"sampler": 0, "target": {"node": 0, "path": "weights"}}],
            "samplers": [{"input": 3, "output": 4, "interpolation": "LINEAR"}]
        }]
    });
    let mut text = serde_json::to_vec(&json).unwrap();
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let mut bin = Vec::new();
    for v in [0f32, 0., 0., 1., 0., 0., 0., 1., 0.] {
        bin.extend(v.to_le_bytes());
    }
    for i in [0u16, 1, 2] {
        bin.extend(i.to_le_bytes());
    }
    bin.extend([0u8; 2]);
    for v in [0f32, 0., 0., 0., 0., 0., 0., 1., 0.] {
        bin.extend(v.to_le_bytes());
    }
    for v in [0f32, 1., 0., 1.] {
        bin.extend(v.to_le_bytes());
    }
    assert_eq!(bin.len(), 96);
    let mut out = Vec::new();
    for word in [
        0x46546c67u32,
        2,
        (12 + 8 + text.len() + 8 + bin.len()) as u32,
        text.len() as u32,
        0x4e4f534a,
    ] {
        out.extend(word.to_le_bytes());
    }
    out.extend(text);
    out.extend((bin.len() as u32).to_le_bytes());
    out.extend(0x004e4942u32.to_le_bytes());
    out.extend(bin);
    out.into()
}

fn open(bytes: Arc<[u8]>) -> AssetDocument {
    import_glb(
        bytes,
        "morph.glb".into(),
        DocumentId(7),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
}

fn lifted(doc: &AssetDocument, time: Option<f32>) -> f32 {
    let cancel = Cancellation::default();
    let scene = match time {
        Some(t) => doc.animated_snapshot(0, t, ImportLimits::default(), &cancel),
        None => doc.snapshot(ImportLimits::default(), &cancel),
    }
    .unwrap();
    scene.positions[2][1]
}

fn save(doc: &AssetDocument) -> Arc<[u8]> {
    prepare_glb(
        doc,
        doc.tag(),
        ExportLimits::default(),
        ImportLimits::default(),
        &Cancellation::default(),
    )
    .unwrap()
    .bytes()
    .clone()
}

#[test]
fn a_morph_target_opens_plays_its_weights_and_saves_unchanged() {
    let source = glb();
    let doc = open(source.clone());
    let morphs = doc.morphs().expect("targets");
    assert_eq!(morphs.summary(), (1, 1));
    let primitive = morphs.primitive(ad_document::MeshId(0)).unwrap();
    assert_eq!(&*primitive.names, ["Lift".to_owned()]);
    assert!(doc.is_read_only());
    let clip = &doc.animation_clips()[0];
    assert!(clip.tracks().is_empty() && clip.weights().len() == 1);
    assert_eq!(clip.duration, 1.);
    // At rest the mesh's weight (0); in the clip the animated weight.
    assert_eq!(lifted(&doc, None), 1.);
    assert!((lifted(&doc, Some(0.5)) - 1.5).abs() < 1e-6);
    assert!((lifted(&doc, Some(1.)) - 2.).abs() < 1e-6);
    // Nothing changed: saving writes the file as it is.
    assert_eq!(&*save(&doc), &*source);
}

#[test]
fn retimed_and_duplicated_clips_keep_their_weights_after_saving() {
    let cancel = Cancellation::default();
    let mut doc = open(glb());
    let apply = |doc: &mut AssetDocument, edit: ClipEdit| {
        let plan = doc.prepare_clip_edit(doc.tag(), &edit, &cancel).unwrap();
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
            .unwrap();
    };
    apply(
        &mut doc,
        ClipEdit::Retime {
            clip: 0,
            duration: 2.,
        },
    );
    assert_eq!(doc.animation_clips()[0].duration, 2.);
    assert!(
        (lifted(&doc, Some(1.)) - 1.5).abs() < 1e-6,
        "half way at 1 s"
    );
    apply(
        &mut doc,
        ClipEdit::Duplicate {
            clip: 0,
            name: "Smile copy".into(),
        },
    );
    let back = open(save(&doc));
    assert_eq!(back.animation_clips().len(), 2);
    for clip in back.animation_clips() {
        assert_eq!(clip.weights().len(), 1);
        assert_eq!(clip.weights()[0].times(), &[0., 2.]);
        assert_eq!(clip.weights()[0].values(), &[0., 1.]);
    }
    assert!(back.morphs().is_some());
    assert!((lifted(&back, Some(1.)) - 1.5).abs() < 1e-6);
    // A transform key joins the morph-only clip and saves too.
    let key = ad_document::AnimationKey {
        clip: 0,
        time: 0.5,
        channel: ad_document::ChannelValue {
            node: NodeId(0),
            kind: ad_document::TrackKind::Translation,
            value: [0., 0., 1., 0.],
        },
        remove: false,
    };
    let plan = doc.prepare_animation_key(doc.tag(), key, &cancel).unwrap();
    doc.apply_triangle_removal(plan, ImportLimits::default(), &cancel)
        .unwrap();
    let back = open(save(&doc));
    let clip = &back.animation_clips()[0];
    assert_eq!((clip.tracks().len(), clip.weights().len()), (1, 1));
    // Joint edits do not apply to a document without a skin.
    assert!(
        doc.prepare_joint_edit(doc.tag(), &JointEdit::Reset, &cancel)
            .is_err()
    );
}
