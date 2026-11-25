//! P7 skeleton overlay: bones and joints of every skin, drawn over the
//! viewport in the pose on screen (rest pose or the shown animation frame).
//! A joint is picked with the left button (the click does not reach object
//! picking); the selection is shared with the Rigging tab's joint tree.
use super::{Actions, ViewerUi};
use ad_document::NodeId;
use egui::{Color32, Stroke};

/// One joint as the application projected it this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayJoint {
    pub node: NodeId,
    pub name: String,
    /// Viewport position (0..1, y down); None when behind the camera.
    pub at: Option<[f32; 2]>,
    /// Index of the parent joint in `SkeletonOverlay::joints`.
    pub parent: Option<usize>,
    /// The joint has TRS channels a clip can key.
    pub animatable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkeletonOverlay {
    pub document: u64,
    pub joints: Vec<OverlayJoint>,
}

/// The joint selected in the skeleton and the Rigging tree.
pub fn joint_id(document: u64) -> egui::Id {
    egui::Id::new(("rig_joint", document))
}
/// The node the Rigging key editor works on (set for animatable joints).
pub fn animation_node_id(document: u64) -> egui::Id {
    egui::Id::new(("animation_node", document))
}

/// Selects `node` for the skeleton, the tree and (when it can be keyed and
/// no clip is playing) the key editor.
pub fn select_joint(
    context: &egui::Context,
    document: u64,
    node: NodeId,
    animatable: bool,
    actions: &mut Actions,
) {
    context.data_mut(|data| {
        data.insert_temp(joint_id(document), node);
        if animatable {
            data.insert_temp(animation_node_id(document), node);
        }
    });
    if animatable {
        actions.animation_preview = Some(None);
    }
}

const BONE: Color32 = Color32::from_rgb(150, 205, 255);
const SELECTED: Color32 = Color32::from_rgb(255, 170, 60);
const OUTLINE: Color32 = Color32::from_rgba_premultiplied(10, 16, 28, 200);

impl ViewerUi {
    pub(super) fn skeleton_ui(
        &mut self,
        context: &egui::Context,
        root: &egui::Ui,
        overlay: &SkeletonOverlay,
        actions: &mut Actions,
    ) {
        let scene = self.scene;
        let at = |p: [f32; 2]| {
            egui::pos2(
                scene.min.x + p[0] * scene.width(),
                scene.min.y + p[1] * scene.height(),
            )
        };
        let selected = context.data(|d| d.get_temp::<NodeId>(joint_id(overlay.document)));
        let selected_index = overlay.joints.iter().position(|j| Some(j.node) == selected);
        // Highlight the selected joint's bones: to its parent and children.
        let in_selection = |index: usize| {
            Some(index) == selected_index
                || overlay.joints[index].parent == selected_index && selected_index.is_some()
        };
        let painter = root.painter().with_clip_rect(scene);
        for (index, joint) in overlay.joints.iter().enumerate() {
            let (Some(parent), Some(end)) = (joint.parent, joint.at) else {
                continue;
            };
            let Some(start) = overlay.joints[parent].at else {
                continue;
            };
            let (a, b) = (at(start), at(end));
            let color = if in_selection(index) { SELECTED } else { BONE };
            // A thin wedge from parent to child, outlined for contrast.
            let dir = (b - a).normalized();
            let side = egui::vec2(-dir.y, dir.x) * 3.;
            let shape = vec![a + side, b, a - side];
            painter.add(egui::Shape::convex_polygon(
                shape.clone(),
                color.gamma_multiply(0.55),
                Stroke::new(1., OUTLINE),
            ));
            painter.line_segment([a, b], Stroke::new(1.5, color));
        }
        for joint in &overlay.joints {
            let Some(p) = joint.at.map(at) else {
                continue;
            };
            if !scene.shrink(2.).contains(p) {
                continue;
            }
            let area = egui::Area::new(egui::Id::new((
                "skeleton_joint",
                overlay.document,
                joint.node.0,
            )))
            .order(egui::Order::Middle)
            .fixed_pos(p - egui::vec2(7., 7.))
            .show(context, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(14., 14.), egui::Sense::click());
                let is_selected = Some(joint.node) == selected;
                let radius = if is_selected {
                    5.5
                } else if response.hovered() {
                    5.
                } else {
                    3.5
                };
                let fill = if is_selected {
                    SELECTED
                } else if response.hovered() {
                    Color32::WHITE
                } else {
                    BONE
                };
                ui.painter()
                    .circle(rect.center(), radius, fill, Stroke::new(1.5, OUTLINE));
                response
            });
            let response = area.inner.on_hover_text(if joint.animatable {
                joint.name.clone()
            } else {
                format!("{} · без TRS-каналов", joint.name)
            });
            if response.clicked() {
                select_joint(
                    context,
                    overlay.document,
                    joint.node,
                    joint.animatable,
                    actions,
                );
            }
        }
    }
}
