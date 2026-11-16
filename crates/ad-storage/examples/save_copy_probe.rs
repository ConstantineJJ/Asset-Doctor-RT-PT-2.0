//! Developer smoke: edit an authored fixture, verify and save to a NEW path.
//! No owner asset is read, modified or replaced.
use ad_document::{Cancellation, DocumentId, ImportLimits, MeshId};
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let destination = PathBuf::from(
        args.next()
            .ok_or("Expected one new absolute .glb destination")?,
    );
    if args.next().is_some() {
        return Err("Expected only one destination".into());
    }
    let cancel = Cancellation::default();
    let limits = ImportLimits::default();
    let mut document = ad_formats::import_glb(
        Arc::from(include_bytes!("../../../tests/fixtures/Geometry_Findings.glb").as_slice()),
        "Save Copy authored probe".into(),
        DocumentId(1),
        limits,
        &cancel,
    )?;
    let before = document.primitives()[0].indices.len() / 3;
    let removal =
        document.prepare_triangle_removal(document.tag(), MeshId(0), &[1], limits, &cancel)?;
    document.apply_triangle_removal(removal, limits, &cancel)?;
    let verified = ad_formats::prepare_glb(
        &document,
        document.tag(),
        Default::default(),
        limits,
        &cancel,
    )?;
    let saved = ad_storage::save_copy(&verified, &document, &destination, &cancel)?;
    println!(
        "SAVE_COPY_PASS source_triangles={before} current_triangles={} verified_bytes={} cleanup_warning={} destination={}",
        document.primitives()[0].indices.len() / 3,
        verified.bytes().len(),
        saved.cleanup_warning(),
        saved.path().display()
    );
    Ok(())
}
