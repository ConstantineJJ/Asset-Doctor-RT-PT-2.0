//! Revision-checked CPU command sessions; no renderer, UI, file IO or repair planner.
use ad_document::{
    AssetDocument, AssetError, AssetResult, Cancellation, CandidateIndices, DocumentTag, ErrorCode,
    ImportLimits, IndexExchange, MeshId, PreparedRemoval, StagedExchange, StagedRemoval,
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
        if plan.changes() {
            self.admit(plan.retained_bytes())?;
        }
        Ok(plan)
    }
    /// ADR-0033: removal of unused vertices of one primitive. History keeps
    /// the whole previous primitive, so admission is checked before the
    /// replacement streams are built.
    pub fn prepare_vertex_removal(
        &self,
        expected: DocumentTag,
        mesh: MeshId,
        vertices: &[u32],
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.document.verify_tag(expected, cancel)?;
        if !vertices.is_empty() {
            self.admit(self.document.primitive_capacity_bytes(mesh)?)?;
        }
        let plan = self.document.prepare_vertex_removal(
            expected,
            mesh,
            vertices,
            self.import_limits,
            cancel,
        )?;
        if plan.changes() {
            self.admit(plan.retained_bytes())?;
        }
        Ok(plan)
    }
    /// ADR-0037: replace one primitive's NORMAL stream (see
    /// `AssetDocument::prepare_normals`). Same admission as vertex edits.
    pub fn prepare_normals(
        &self,
        expected: DocumentTag,
        mesh: MeshId,
        normals: Vec<[f32; 3]>,
        keep_authored: bool,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.document.verify_tag(expected, cancel)?;
        self.admit(self.document.primitive_capacity_bytes(mesh)?)?;
        let plan = self.document.prepare_normals(
            expected,
            mesh,
            normals,
            keep_authored,
            self.import_limits,
            cancel,
        )?;
        if plan.changes() {
            self.admit(plan.retained_bytes())?;
        }
        Ok(plan)
    }
    /// ADR-0034: merge exact duplicate vertices, `[removed, kept]` pairs.
    /// Same history admission as a vertex removal.
    pub fn prepare_vertex_merge(
        &self,
        expected: DocumentTag,
        mesh: MeshId,
        pairs: &[[u32; 2]],
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.document.verify_tag(expected, cancel)?;
        if !pairs.is_empty() {
            self.admit(self.document.primitive_capacity_bytes(mesh)?)?;
        }
        let plan = self.document.prepare_vertex_merge(
            expected,
            mesh,
            pairs,
            self.import_limits,
            cancel,
        )?;
        if plan.changes() {
            self.admit(plan.retained_bytes())?;
        }
        Ok(plan)
    }
    /// ADR-0041: replace the model placement. History keeps the previous one.
    pub fn prepare_placement(
        &self,
        expected: DocumentTag,
        placement: ad_document::Placement,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.document.verify_tag(expected, cancel)?;
        let plan = self
            .document
            .prepare_placement(expected, placement, cancel)?;
        if plan.changes() {
            self.admit(plan.retained_bytes())?;
        }
        Ok(plan)
    }
    /// ADR-0048: one object edit (node transform or taking a node out).
    pub fn prepare_node_op(
        &self,
        expected: DocumentTag,
        op: ad_document::NodeOp,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.document.verify_tag(expected, cancel)?;
        let plan = self.document.prepare_node_op(expected, op, cancel)?;
        if plan.changes() {
            self.admit(plan.retained_bytes())?;
        }
        Ok(plan)
    }
    pub fn prepare_animation_key(
        &self,
        expected: DocumentTag,
        key: ad_document::AnimationKey,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        let plan = self.document.prepare_animation_key(expected, key, cancel)?;
        if plan.changes() {
            self.admit(plan.retained_bytes())?;
        }
        Ok(plan)
    }
    pub fn commit(
        &mut self,
        plan: PreparedRemoval,
        cancel: &Cancellation,
    ) -> AssetResult<CommandOutcome> {
        if !plan.changes() {
            self.document.verify_tag(plan.preview().tag, cancel)?;
            let applied = self
                .document
                .apply_triangle_removal(plan, self.import_limits, cancel)?;
            return Ok(CommandOutcome {
                tag: applied.tag,
                changed: false,
            });
        }
        Ok(CommandOutcome {
            tag: self.stage_commit(plan, cancel)?.publish(),
            changed: true,
        })
    }
    /// Every history and document check of a new command, without mutation
    /// (ADR-0031). Dropping the stage keeps document, cursor and redo intact;
    /// only `StagedEdit::publish` discards redo. No-op plans are refused.
    pub fn stage_commit(
        &mut self,
        plan: PreparedRemoval,
        cancel: &Cancellation,
    ) -> AssetResult<StagedEdit<'_>> {
        self.document.verify_tag(plan.preview().tag, cancel)?;
        let payload = self.admit(plan.retained_bytes())?;
        let next = self.next_state.checked_add(1).ok_or_else(budget)?;
        let before = self.state;
        let Self {
            document,
            history,
            cursor,
            state,
            next_state,
            payload_bytes,
            import_limits,
            ..
        } = self;
        let change = document.stage_triangle_removal(plan, *import_limits, cancel)?;
        Ok(StagedEdit(Stage::Commit {
            change,
            history,
            cursor,
            state,
            next_state,
            payload_bytes,
            payload,
            before,
            next,
        }))
    }
    pub fn undo(
        &mut self,
        expected: DocumentTag,
        cancel: &Cancellation,
    ) -> AssetResult<DocumentTag> {
        Ok(self.stage_undo(expected, cancel)?.publish())
    }
    pub fn redo(
        &mut self,
        expected: DocumentTag,
        cancel: &Cancellation,
    ) -> AssetResult<DocumentTag> {
        Ok(self.stage_redo(expected, cancel)?.publish())
    }
    pub fn stage_undo(
        &mut self,
        expected: DocumentTag,
        cancel: &Cancellation,
    ) -> AssetResult<StagedEdit<'_>> {
        self.document.verify_tag(expected, cancel)?;
        let index = self.cursor.checked_sub(1).ok_or_else(empty)?;
        self.stage_move(expected, index, index, false, cancel)
    }
    pub fn stage_redo(
        &mut self,
        expected: DocumentTag,
        cancel: &Cancellation,
    ) -> AssetResult<StagedEdit<'_>> {
        self.document.verify_tag(expected, cancel)?;
        let index = self.cursor;
        if index >= self.history.len() {
            return Err(empty());
        }
        self.stage_move(expected, index, index + 1, true, cancel)
    }
    fn stage_move(
        &mut self,
        expected: DocumentTag,
        index: usize,
        cursor_to: usize,
        forward: bool,
        cancel: &Cancellation,
    ) -> AssetResult<StagedEdit<'_>> {
        let Self {
            document,
            history,
            cursor,
            state,
            import_limits,
            ..
        } = self;
        let entry = &mut history[index];
        let state_to = if forward { entry.after } else { entry.before };
        let change =
            document.stage_exchange(expected, &mut entry.change, *import_limits, cancel)?;
        Ok(StagedEdit(Stage::Move {
            change,
            cursor,
            state,
            cursor_to,
            state_to,
        }))
    }
    /// Read-only indices the next Undo would publish, tagged with its revision.
    pub fn undo_candidate(&self) -> AssetResult<CandidateIndices<'_>> {
        let index = self.cursor.checked_sub(1).ok_or_else(empty)?;
        self.history[index].change.candidate(&self.document)
    }
    /// Read-only indices the next Redo would publish, tagged with its revision.
    pub fn redo_candidate(&self) -> AssetResult<CandidateIndices<'_>> {
        self.history
            .get(self.cursor)
            .ok_or_else(empty)?
            .change
            .candidate(&self.document)
    }
}

/// A fully checked command, Undo or Redo. It borrows the Session mutably, so
/// nothing can change between the checks and `publish` (ADR-0031).
pub struct StagedEdit<'a>(Stage<'a>);
enum Stage<'a> {
    Commit {
        change: StagedRemoval<'a>,
        history: &'a mut Vec<Entry>,
        cursor: &'a mut usize,
        state: &'a mut u64,
        next_state: &'a mut u64,
        payload_bytes: &'a mut usize,
        payload: usize,
        before: u64,
        next: u64,
    },
    Move {
        change: StagedExchange<'a>,
        cursor: &'a mut usize,
        state: &'a mut u64,
        cursor_to: usize,
        state_to: u64,
    },
}
impl StagedEdit<'_> {
    pub fn tag_after(&self) -> DocumentTag {
        match &self.0 {
            Stage::Commit { change, .. } => change.tag_after(),
            Stage::Move { change, .. } => change.tag_after(),
        }
    }
    /// Infallible publication: buffer swap, revision, cursor/state, and for a
    /// new command redo truncation plus a push into capacity reserved at
    /// Session construction and checked by admission.
    pub fn publish(self) -> DocumentTag {
        match self.0 {
            Stage::Commit {
                change,
                history,
                cursor,
                state,
                next_state,
                payload_bytes,
                payload,
                before,
                next,
            } => {
                let tag = change.tag_after();
                let change = change.publish();
                history.truncate(*cursor);
                debug_assert!(history.len() < history.capacity());
                history.push(Entry {
                    change,
                    before,
                    after: next,
                });
                *cursor += 1;
                *state = next;
                *next_state = next;
                *payload_bytes = payload;
                tag
            }
            Stage::Move {
                change,
                cursor,
                state,
                cursor_to,
                state_to,
            } => {
                let tag = change.publish();
                *cursor = cursor_to;
                *state = state_to;
                tag
            }
        }
    }
}

#[cfg(test)]
mod stage_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod vertex_tests;
