//! Revision-checked CPU command sessions; no renderer, UI, file IO or repair planner.
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode, ImportLimits,
    IndexExchange, MeshId, PreparedRemoval,
};

#[derive(Clone, Copy)]
pub struct HistoryLimits {
    pub bytes: usize,
    pub entries: usize,
}
impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            bytes: 64 << 20,
            entries: 128,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryUsage {
    pub bytes: usize,
    pub metadata_bytes: usize,
    pub undo: usize,
    pub redo: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandOutcome {
    pub tag: DocumentTag,
    pub changed: bool,
}
struct Entry {
    change: IndexExchange,
    before: u64,
    after: u64,
}
pub struct Session {
    document: AssetDocument,
    history: Vec<Entry>,
    cursor: usize,
    state: u64,
    next_state: u64,
    payload_bytes: usize,
    metadata_bytes: usize,
    history_limits: HistoryLimits,
    import_limits: ImportLimits,
}
fn budget() -> AssetError {
    AssetError::new(ErrorCode::ResourceLimit, "Command history limit reached")
}
fn empty() -> AssetError {
    AssetError::new(
        ErrorCode::HistoryEmpty,
        "No command is available in this history direction",
    )
}

impl Session {
    /// Admits only a fresh revision-zero candidate and consumes it on failure.
    /// Desktop must keep its current asset until candidate admission succeeds.
    pub fn new(
        document: AssetDocument,
        history_limits: HistoryLimits,
        import_limits: ImportLimits,
    ) -> AssetResult<Self> {
        if document.revision() != 0 {
            return Err(AssetError::new(
                ErrorCode::Conflict,
                "A command session requires a fresh imported document",
            ));
        }
        let mut history = Vec::new();
        let slots = history_limits
            .entries
            .min(history_limits.bytes / std::mem::size_of::<Entry>());
        history.try_reserve_exact(slots).map_err(|_| budget())?;
        let metadata_bytes = history
            .capacity()
            .checked_mul(std::mem::size_of::<Entry>())
            .ok_or_else(budget)?;
        if metadata_bytes > history_limits.bytes {
            return Err(budget());
        }
        Ok(Self {
            document,
            history,
            cursor: 0,
            state: 0,
            next_state: 0,
            payload_bytes: 0,
            metadata_bytes,
            history_limits,
            import_limits,
        })
    }
    pub fn document(&self) -> &AssetDocument {
        &self.document
    }
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }
    pub fn can_redo(&self) -> bool {
        self.cursor < self.history.len()
    }
    /// Relative to imported content, not proof of saved/unsaved file state.
    pub fn is_modified(&self) -> bool {
        self.state != 0
    }
    pub fn history_usage(&self) -> HistoryUsage {
        HistoryUsage {
            bytes: self.metadata_bytes + self.payload_bytes,
            metadata_bytes: self.metadata_bytes,
            undo: self.cursor,
            redo: self.history.len() - self.cursor,
        }
    }
    fn admit(&self, payload: usize) -> AssetResult<usize> {
        if self.cursor >= self.history_limits.entries || self.cursor >= self.history.capacity() {
            return Err(budget());
        }
        let prefix = self.history[..self.cursor]
            .iter()
            .try_fold(0usize, |sum, entry| {
                sum.checked_add(entry.change.retained_bytes())
            })
            .ok_or_else(budget)?;
        let projected = prefix.checked_add(payload).ok_or_else(budget)?;
        if projected
            .checked_add(self.metadata_bytes)
            .is_none_or(|bytes| bytes > self.history_limits.bytes)
        {
            return Err(budget());
        }
        Ok(projected)
    }
    pub fn prepare_removal(
        &self,
        expected: DocumentTag,
        mesh: MeshId,
        triangles: &[u32],
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.document.verify_tag(expected, cancel)?;
        // Refuse a full history before constructing a potentially large candidate.
        if !triangles.is_empty() {
            self.admit(self.document.index_capacity_bytes(mesh)?)?;
        }
        let plan = self.document.prepare_triangle_removal(
            expected,
            mesh,
            triangles,
            self.import_limits,
            cancel,
        )?;
        if plan.preview().before_triangles != plan.preview().after_triangles {
            self.admit(plan.retained_bytes())?;
        }
        Ok(plan)
    }
    pub fn commit(
        &mut self,
        plan: PreparedRemoval,
        cancel: &Cancellation,
    ) -> AssetResult<CommandOutcome> {
        self.document.verify_tag(plan.preview().tag, cancel)?;
        if plan.preview().before_triangles == plan.preview().after_triangles {
            let applied = self
                .document
                .apply_triangle_removal(plan, self.import_limits, cancel)?;
            return Ok(CommandOutcome {
                tag: applied.tag,
                changed: false,
            });
        }
        let payload = self.admit(plan.retained_bytes())?;
        let next = self.next_state.checked_add(1).ok_or_else(budget)?;
        let applied = self
            .document
            .apply_triangle_removal(plan, self.import_limits, cancel)?;
        if let Some(change) = applied.change {
            // Capacity was reserved at construction; no allocation/fallible work
            // remains after document commit. Discard redo only on actual success.
            self.history.truncate(self.cursor);
            self.history.push(Entry {
                change,
                before: self.state,
                after: next,
            });
            self.cursor += 1;
            self.state = next;
            self.next_state = next;
            self.payload_bytes = payload;
        }
        Ok(CommandOutcome {
            tag: applied.tag,
            changed: true,
        })
    }
    pub fn undo(
        &mut self,
        expected: DocumentTag,
        cancel: &Cancellation,
    ) -> AssetResult<DocumentTag> {
        self.document.verify_tag(expected, cancel)?;
        let index = self.cursor.checked_sub(1).ok_or_else(empty)?;
        let entry = &mut self.history[index];
        let tag = self.document.exchange_indices(
            expected,
            &mut entry.change,
            self.import_limits,
            cancel,
        )?;
        self.state = entry.before;
        self.cursor = index;
        Ok(tag)
    }
    pub fn redo(
        &mut self,
        expected: DocumentTag,
        cancel: &Cancellation,
    ) -> AssetResult<DocumentTag> {
        self.document.verify_tag(expected, cancel)?;
        let entry = self.history.get_mut(self.cursor).ok_or_else(empty)?;
        let tag = self.document.exchange_indices(
            expected,
            &mut entry.change,
            self.import_limits,
            cancel,
        )?;
        self.state = entry.after;
        self.cursor += 1;
        Ok(tag)
    }
}

#[cfg(test)]
mod tests;
