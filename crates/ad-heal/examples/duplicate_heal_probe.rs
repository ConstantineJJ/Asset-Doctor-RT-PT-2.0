//! Authored CPU plan/apply/analysis/Undo/Redo/verified-new-copy developer probe.
//! No owner assets or native editing controls are involved.
use ad_document::{Cancellation, DocumentId, ImportLimits, MeshId};
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("Expected one NEW absolute .glb path")?);
    if args.next().is_some() {
        return Err("Expected one destination".into());
    }
    let c = Cancellation::default();
    let limits = ImportLimits::default();
    let doc = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Duplicate_Heal.glb").as_slice()),
        "Duplicate Heal probe".into(),
        DocumentId(1),
        limits,
        &c,
    )?;
    let source = doc.source().clone();
    let original = doc.primitives()[0].indices.clone();
    let mut session = ad_commands::Session::new(doc, Default::default(), limits)?;
    let plan = ad_heal::plan_duplicates(
        session.document(),
        session.document().tag(),
        MeshId(0),
        Default::default(),
        &c,
    )?;
    let preview = plan.preview();
    assert_eq!(plan.removed_triangles().len(), 32);
    session.commit(plan.prepare(&session, Default::default(), &c)?, &c)?;
    let report = ad_analysis::analyze(session.document(), Default::default(), &c)?;
    assert!(report.is_complete());
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.mesh == MeshId(0) && f.code == ad_analysis::Code::DuplicateTriangle)
    );
    session.undo(session.document().tag(), &c)?;
    assert_eq!(session.document().primitives()[0].indices, original);
    session.redo(session.document().tag(), &c)?;
    let proof = ad_formats::prepare_glb(
        session.document(),
        session.document().tag(),
        Default::default(),
        limits,
        &c,
    )?;
    let saved = ad_storage::save_copy(&proof, session.document(), &path, &c)?;
    assert!(saved.is_current(session.document()));
    assert!(Arc::ptr_eq(session.document().source(), &source));
    println!(
        "DUPLICATE_HEAL_PASS before={} after={} source_node_references={} removed=32 revision={} undo_redo=true source_preserved=true verified_bytes={} cleanup_warning={} path={}",
        preview.before_triangles,
        preview.after_triangles,
        preview.source_node_references,
        session.document().revision(),
        proof.bytes().len(),
        saved.cleanup_warning(),
        saved.path().display()
    );
    Ok(())
}
