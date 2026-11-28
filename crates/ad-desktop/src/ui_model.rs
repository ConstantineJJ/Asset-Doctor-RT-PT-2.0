use ad_document::{AssetDocument, DocumentId, NodeId};

pub struct SceneRow {
    pub id: NodeId,
    pub depth: usize,
    pub label: String,
}
#[derive(Default)]
pub struct SceneTree {
    pub document: Option<DocumentId>,
    pub rows: Vec<SceneRow>,
    /// The primary selection (Info, object gizmo, last clicked).
    pub selected: Option<NodeId>,
    /// Every selected node, the primary one included (multi-selection:
    /// box, Shift adds, Ctrl removes). Use the methods below to change it.
    pub selection: Vec<NodeId>,
    /// Work-Tree view state (node indices): hidden nodes and colour marks;
    /// `rev` changes with them. Cleared for another document.
    pub hidden: std::collections::BTreeSet<usize>,
    pub tints: std::collections::BTreeMap<usize, [u8; 3]>,
    pub rev: u64,
}
/// Hidden and mark of every node: a node follows its nearest marked
/// ancestor (itself included); hidden when it or any ancestor is hidden.
pub fn node_marks(document: &AssetDocument, tree: &SceneTree) -> Vec<(bool, Option<[u8; 3]>)> {
    let nodes = document.nodes();
    let mut marks = vec![(false, None); nodes.len()];
    let mut pending: Vec<(usize, bool, Option<[u8; 3]>)> = document
        .roots()
        .iter()
        .map(|id| (id.0, false, None))
        .collect();
    let mut seen = vec![false; nodes.len()];
    while let Some((id, hidden, tint)) = pending.pop() {
        if id >= nodes.len() || seen[id] {
            continue;
        }
        seen[id] = true;
        let hidden = hidden || tree.hidden.contains(&id);
        let tint = tree.tints.get(&id).copied().or(tint);
        marks[id] = (hidden, tint);
        pending.extend(nodes[id].children.iter().map(|c| (c.0, hidden, tint)));
    }
    marks
}
impl SceneTree {
    /// One node selected (or none).
    pub fn select_only(&mut self, node: Option<NodeId>) {
        self.selected = node;
        self.selection = node.into_iter().collect();
    }
    /// Adds a node and makes it the primary one.
    pub fn select_add(&mut self, node: NodeId) {
        self.sync();
        if !self.selection.contains(&node) {
            self.selection.push(node);
        }
        self.selected = Some(node);
    }
    /// Removes a node; the primary moves to the last remaining one.
    pub fn select_remove(&mut self, node: NodeId) {
        self.sync();
        self.selection.retain(|n| *n != node);
        if self.selected == Some(node) {
            self.selected = self.selection.last().copied();
        }
    }
    /// Replaces the selection; the last node becomes the primary one.
    pub fn select_many(&mut self, nodes: Vec<NodeId>) {
        self.selection = nodes;
        self.selection.dedup();
        self.selected = self.selection.last().copied();
    }
    pub fn is_selected(&self, node: NodeId) -> bool {
        self.selected == Some(node) || self.selection.contains(&node)
    }
    /// Every selected node (primary included), in selection order.
    pub fn all_selected(&self) -> Vec<NodeId> {
        let mut all = self.selection.clone();
        if let Some(primary) = self.selected
            && !all.contains(&primary)
        {
            all.push(primary);
        }
        all
    }
    /// Keeps `selection` consistent after `selected` was set directly.
    fn sync(&mut self) {
        if let Some(primary) = self.selected
            && !self.selection.contains(&primary)
        {
            self.selection = vec![primary];
        }
    }
    pub fn replace(&mut self, document: &AssetDocument) {
        if self.document == Some(document.id()) {
            return;
        }
        self.document = Some(document.id());
        self.selected = None;
        self.selection.clear();
        self.hidden.clear();
        self.tints.clear();
        self.refresh(document);
    }
    /// Rebuilds the rows of the same document (RIG-03: renamed or added
    /// nodes), keeping selection, hidden nodes and marks.
    pub fn refresh(&mut self, document: &AssetDocument) {
        let count = document.nodes().len();
        self.rows.clear();
        self.rev += 1;
        self.selection.retain(|n| n.0 < count);
        if self.selected.is_some_and(|n| n.0 >= count) {
            self.selected = self.selection.last().copied();
        }
        self.hidden.retain(|n| *n < count);
        self.tints.retain(|n, _| *n < count);
        let mut seen = vec![false; document.nodes().len()];
        let mut pending: Vec<_> = document
            .roots()
            .iter()
            .rev()
            .map(|id| (*id, 0usize))
            .collect();
        while let Some((id, depth)) = pending.pop() {
            // Prepared documents already passed snapshot validation; keep this
            // display projection bounded even if reused with an invalid graph.
            if id.0 >= seen.len() || seen[id.0] {
                continue;
            }
            seen[id.0] = true;
            let node = &document.nodes()[id.0];
            let label = node
                .name
                .chars()
                .filter(|c| !c.is_control())
                .take(256)
                .collect();
            self.rows.push(SceneRow { id, depth, label });
            pending.extend(node.children.iter().rev().map(|id| (*id, depth + 1)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ad_document::{Cancellation, ImportLimits};
    use std::sync::Arc;
    fn document(id: u64) -> AssetDocument {
        ad_formats::import_glb(
            Arc::from(include_bytes!("../../../tests/fixtures/Twin_Cubes.glb").as_slice()),
            "Twin_Cubes.glb".into(),
            DocumentId(id),
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap()
    }
    #[test]
    fn hierarchy_is_preordered_and_selection_belongs_to_one_document() {
        let mut tree = SceneTree::default();
        tree.replace(&document(1));
        assert_eq!(
            tree.rows
                .iter()
                .map(|r| (r.id.0, r.depth))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 1), (2, 1)]
        );
        tree.selected = Some(NodeId(1));
        tree.replace(&document(1));
        assert_eq!(tree.selected, Some(NodeId(1)));
        tree.replace(&document(2));
        assert_eq!(tree.selected, None);
    }
    #[test]
    fn multi_selection_adds_removes_and_keeps_a_primary() {
        let mut tree = SceneTree::default();
        tree.select_only(Some(NodeId(1)));
        tree.select_add(NodeId(2));
        tree.select_add(NodeId(3));
        tree.select_add(NodeId(2));
        assert_eq!(tree.all_selected(), [NodeId(1), NodeId(2), NodeId(3)]);
        assert_eq!(tree.selected, Some(NodeId(2)));
        tree.select_remove(NodeId(2));
        assert_eq!(tree.all_selected(), [NodeId(1), NodeId(3)]);
        assert_eq!(tree.selected, Some(NodeId(3)));
        assert!(tree.is_selected(NodeId(1)) && !tree.is_selected(NodeId(2)));
        // Setting the primary directly starts a new selection on the next add.
        tree.selected = Some(NodeId(5));
        tree.select_add(NodeId(6));
        assert_eq!(tree.all_selected(), [NodeId(5), NodeId(6)]);
        tree.select_many(vec![NodeId(7), NodeId(8)]);
        assert_eq!(tree.selected, Some(NodeId(8)));
        tree.select_only(None);
        assert!(tree.all_selected().is_empty());
    }
    #[test]
    fn hidden_and_marks_follow_the_parent_and_reset_for_another_document() {
        let doc = document(1);
        let mut tree = SceneTree::default();
        tree.replace(&doc);
        // Twin_Cubes: node 0 with children 1 and 2.
        tree.tints.insert(0, [1, 2, 3]);
        tree.tints.insert(2, [9, 9, 9]);
        tree.hidden.insert(1);
        let marks = node_marks(&doc, &tree);
        assert_eq!(marks[0], (false, Some([1, 2, 3])));
        assert_eq!(
            marks[1],
            (true, Some([1, 2, 3])),
            "child follows the parent mark"
        );
        assert_eq!(marks[2], (false, Some([9, 9, 9])), "its own mark wins");
        tree.hidden.insert(0);
        assert!(
            node_marks(&doc, &tree).iter().all(|m| m.0),
            "hiding the root hides all"
        );
        // Snapshot draws name their nodes.
        let snapshot = doc
            .snapshot(ImportLimits::default(), &Cancellation::default())
            .unwrap();
        let mut nodes: Vec<_> = snapshot.draws.iter().map(|d| d.node.0).collect();
        nodes.dedup();
        assert_eq!(nodes, [1, 2]);
        let rev = tree.rev;
        tree.replace(&document(2));
        assert!(tree.hidden.is_empty() && tree.tints.is_empty() && tree.rev > rev);
    }
    #[test]
    fn deep_hierarchy_is_flattened_iteratively_without_source_changes() {
        use ad_document::{DocumentParts, Node};
        let count = 10_000;
        let nodes = (0..count)
            .map(|i| Node {
                name: "node".into(),
                local: glam::Mat4::IDENTITY.to_cols_array_2d(),
                children: if i + 1 < count {
                    vec![NodeId(i + 1)]
                } else {
                    vec![]
                },
                meshes: Arc::from([]),
            })
            .collect();
        let document = AssetDocument::new(
            DocumentParts {
                id: DocumentId(1),
                name: "deep".into(),
                source: Arc::from([1, 2, 3]),
                nodes,
                meshes: vec![],
                roots: vec![NodeId(0)],
                notes: vec![],
            },
            ImportLimits::default(),
            &Cancellation::default(),
        )
        .unwrap();
        let mut tree = SceneTree::default();
        tree.replace(&document);
        assert_eq!(tree.rows.len(), count);
        assert_eq!(tree.rows.last().unwrap().depth, count - 1);
        assert_eq!(&**document.source(), [1, 2, 3]);
    }
}
