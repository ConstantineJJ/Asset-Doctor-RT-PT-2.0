//! Save Copy, receipt-based dirty state and unsaved-change guard (ADR-0031 D).
//!
//! Save Copy writes a verified GLB to a new path only (ADR-0028/0029), then
//! independently reads the published file back from disk and reimports it.
//! The working path never changes; a receipt clears "unsaved" only for the
//! exact document revision/origin it was saved from (decision 2).
use crate::imports::PreparedAsset;
use crate::worker::{Request, Worker};
use ad_commands::Session;
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentId, DocumentStamp, ErrorCode,
    ImportLimits,
};
use ad_storage::SavedCopy;
use std::{io::Read, path::PathBuf, sync::Arc};

pub struct SaveInput {
    pub session: Arc<Session>,
    pub base: DocumentStamp,
    pub path: PathBuf,
}
pub struct SaveOutcome {
    pub receipt: SavedCopy,
    pub bytes: usize,
    /// Unchanged content: the original source bytes were copied exactly.
    pub exact_copy: bool,
    pub reimported_triangles: usize,
    pub cleanup_warning: bool,
}

fn read_back(path: &std::path::Path, limit: usize) -> AssetResult<Vec<u8>> {
    let io =
        |e: std::io::Error| AssetError::new(ErrorCode::Io, format!("Saved copy readback: {e}"));
    let mut file = std::fs::File::open(path).map_err(io)?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > limit {
        return Err(AssetError::new(
            ErrorCode::ResourceLimit,
            "Saved copy exceeds the import source budget",
        ));
    }
    Ok(bytes)
}

/// Verified export → new-path storage → fresh disk read and reimport.
pub fn save_copy_job(input: &SaveInput, cancel: &Cancellation) -> AssetResult<SaveOutcome> {
    cancel.check()?;
    let document = input.session.document();
    if !input.base.is_current(document) {
        return Err(AssetError::new(
            ErrorCode::Conflict,
            "The model changed before Save Copy started",
        ));
    }
    let limits = ImportLimits::default();
    let verified = ad_formats::prepare_glb(
        document,
        input.base.tag(),
        ad_formats::ExportLimits::default(),
        limits,
        cancel,
    )?;
    let exact_copy = Arc::ptr_eq(verified.bytes(), document.source());
    let bytes = verified.bytes().len();
    let receipt = ad_storage::save_copy(&verified, document, &input.path, cancel)?;
    // The file is published. A late cancel must not turn that into failure,
    // so the independent readback uses its own token.
    let after = Cancellation::default();
    let saved = read_back(receipt.path(), limits.source_bytes)?;
    if saved.as_slice() != verified.bytes().as_ref() {
        return Err(AssetError::new(
            ErrorCode::VerificationFailed,
            format!(
                "Saved copy {} differs from the verified bytes; it is not treated as saved",
                receipt.path().display()
            ),
        ));
    }
    let reimported = ad_formats::import_glb(
        saved.into(),
        document.name().into(),
        DocumentId(u64::MAX),
        limits,
        &after,
    )?;
    ad_formats::compare_reimported(document, &reimported, &after).map_err(|e| {
        AssetError::new(
            ErrorCode::VerificationFailed,
            format!(
                "Saved copy {} failed independent reimport ({}); it is not treated as saved",
                receipt.path().display(),
                e.message
            ),
        )
    })?;
    let reimported_triangles = reimported
        .primitives()
        .iter()
        .map(|m| m.indices.len() / 3)
        .sum();
    Ok(SaveOutcome {
        cleanup_warning: receipt.cleanup_warning(),
        receipt,
        bytes,
        exact_copy,
        reimported_triangles,
    })
}

pub type Saves = Worker<SaveInput, SaveOutcome>;
pub fn new_saves() -> Saves {
    Worker::new(
        |request: &Request<SaveInput>| save_copy_job(&request.input, &request.cancel),
        "Save Copy stopped unexpectedly; the model was not marked as saved",
    )
}

/// The last state known to exist on disk: the imported file or a verified copy.
pub enum Confirmed {
    Import(DocumentStamp),
    Saved(SavedCopy),
}
impl Confirmed {
    pub fn imported(asset: &PreparedAsset) -> Self {
        Self::Import(asset.document().stamp())
    }
    fn is_current(&self, document: &AssetDocument) -> bool {
        match self {
            Self::Import(stamp) => stamp.is_current(document),
            Self::Saved(receipt) => receipt.is_current(document),
        }
    }
    /// Revision or origin differs from the confirmed state. Conservative:
    /// returning to equal content through Undo still counts as unsaved.
    pub fn is_dirty(&self, asset: &PreparedAsset) -> bool {
        !self.is_current(asset.document())
    }
    /// Accepts a receipt only for the current revision/origin.
    pub fn accept(&mut self, receipt: SavedCopy, asset: &PreparedAsset) -> bool {
        if receipt.is_current(asset.document()) {
            *self = Self::Saved(receipt);
            true
        } else {
            false
        }
    }
}

/// What to do after the unsaved-change guard (and an optional Save Copy).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AfterSave {
    Close,
    Choose,
    Open(PathBuf),
}
impl AfterSave {
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Close => "закрыть программу",
            Self::Choose | Self::Open(_) => "открыть другую модель",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsavedChoice {
    SaveCopy,
    Discard,
    Cancel,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guard {
    Proceed,
    Ask,
    Stay,
    SaveThenProceed,
}
/// Policy before close or replacing the model.
pub fn guard(dirty: bool, choice: Option<UnsavedChoice>) -> Guard {
    match (dirty, choice) {
        (false, _) => Guard::Proceed,
        (true, None) => Guard::Ask,
        (true, Some(UnsavedChoice::Discard)) => Guard::Proceed,
        (true, Some(UnsavedChoice::Cancel)) => Guard::Stay,
        (true, Some(UnsavedChoice::SaveCopy)) => Guard::SaveThenProceed,
    }
}

#[cfg(test)]
#[path = "saving_tests.rs"]
mod saving_tests;
