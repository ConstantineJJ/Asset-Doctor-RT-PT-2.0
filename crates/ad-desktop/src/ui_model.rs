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
    pub selected: Option<NodeId>,
}
impl SceneTree {
    pub fn replace(&mut self, document: &AssetDocument) {
        if self.document == Some(document.id()) {
            return;
        }
        self.document = Some(document.id());
        self.selected = None;
        self.rows.clear();
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
