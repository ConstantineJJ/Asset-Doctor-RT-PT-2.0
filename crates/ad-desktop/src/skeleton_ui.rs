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
        document: Option<&ad_document::AssetDocument>,
        actions: &mut Actions,
    ) {
        let controls = self.playback;
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
            if response.hovered() {
                self.pointer_on_gizmo = true;
            }
            if response.clicked() {
                select_joint(
                    context,
                    overlay.document,
                    joint.node,
                    joint.animatable,
                    actions,
                );
            }
            // Right click: select it and key its pose (Rigging menu).
            if let Some(document) = document.filter(|d| d.id().0 == overlay.document) {
                response.context_menu(|ui| {
                    super::rig_ui::joint_menu(ui, document, joint.node, controls, actions);
                });
            }
        }
    }
}

/// A part of the joint gizmo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JointHandle {
    Ring(usize),
    Arrow(usize),
}

const AXIS_COLORS: [Color32; 3] = [
    Color32::from_rgb(235, 70, 70),
    Color32::from_rgb(90, 210, 90),
    Color32::from_rgb(80, 140, 245),
];

fn segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0., 1.);
    (p - (a + ab * t)).length()
}

impl ViewerUi {
    /// Rings turn the joint, arrows move it: preview while dragging, one key
    /// of the current clip on release.
    pub(super) fn joint_gizmo_ui(
        &mut self,
        context: &egui::Context,
        root: &egui::Ui,
        gizmo: &crate::joint_gizmo::JointGizmo,
        actions: &mut Actions,
    ) {
        use ad_document::{AnimationKey, ChannelValue, TrackKind};
        let scene = self.scene;
        let at = |p: [f32; 2]| {
            egui::pos2(
                scene.min.x + p[0] * scene.width(),
                scene.min.y + p[1] * scene.height(),
            )
        };
        let uv = |p: egui::Pos2| {
            [
                (p.x - scene.min.x) / scene.width(),
                (p.y - scene.min.y) / scene.height(),
            ]
        };
        let aspect = scene.width() / scene.height().max(1.);
        let painter = root.painter().with_clip_rect(scene);
        let pointer = context.pointer_hover_pos();
        let centre = at(gizmo.center);
        // Dragged handle first, so a ring under an arrow does not steal it.
        let mut handles: Vec<(JointHandle, egui::Pos2)> = Vec::new();
        for axis in 0..3 {
            let ring = &gizmo.rings[axis];
            let hot = self
                .joint_drag
                .is_some_and(|d| d.0 == JointHandle::Ring(axis))
                || pointer.is_some_and(|p| {
                    (0..ring.len()).any(|i| match (ring[i], ring[(i + 1) % ring.len()]) {
                        (Some(a), Some(b)) => segment_distance(p, at(a), at(b)) < 6.,
                        _ => false,
                    })
                });
            for i in 0..ring.len() {
                if let (Some(a), Some(b)) = (ring[i], ring[(i + 1) % ring.len()]) {
                    painter.line_segment(
                        [at(a), at(b)],
                        egui::Stroke::new(if hot { 3. } else { 1.6 }, AXIS_COLORS[axis]),
                    );
                }
            }
            if hot && let Some(p) = pointer {
                handles.push((JointHandle::Ring(axis), p));
            }
            if let Some(tip) = gizmo.arrows[axis].map(at) {
                painter.line_segment([centre, tip], egui::Stroke::new(2., AXIS_COLORS[axis]));
                handles.push((JointHandle::Arrow(axis), tip));
            }
        }
        painter.circle_filled(centre, 3., Color32::WHITE);
        self.pointer_on_gizmo |= handles.iter().any(|(h, p)| match h {
            JointHandle::Ring(_) => true,
            JointHandle::Arrow(_) => pointer.is_some_and(|q| q.distance(*p) < 12.),
        });
        for (handle, position) in handles {
            let id = egui::Id::new(("joint_gizmo", gizmo.document, format!("{handle:?}")));
            let area = egui::Area::new(id.with("area"))
                .order(egui::Order::Middle)
                .fixed_pos(position - egui::vec2(10., 10.))
                .show(context, |ui| {
                    let rect = egui::Rect::from_center_size(position, egui::vec2(20., 20.));
                    let response = ui.interact(rect, id, egui::Sense::drag());
                    if let JointHandle::Arrow(axis) = handle {
                        let r = if response.hovered() || response.dragged() {
                            7.
                        } else {
                            5.
                        };
                        ui.painter().circle(
                            rect.center(),
                            r,
                            AXIS_COLORS[axis],
                            egui::Stroke::new(1.5, OUTLINE),
                        );
                    }
                    response
                });
            let response = area.inner;
            if response.drag_started()
                && let Some(p) = response.interact_pointer_pos()
            {
                let base = match handle {
                    JointHandle::Ring(_) => gizmo.rotation,
                    JointHandle::Arrow(_) => {
                        let t = gizmo.translation;
                        [t[0], t[1], t[2], 0.]
                    }
                };
                self.joint_drag = Some((handle, uv(p), base));
            }
            if response.dragged()
                && let (Some(p), Some((h, start, _))) =
                    (response.interact_pointer_pos(), self.joint_drag)
                && h == handle
            {
                let (kind, value) = match handle {
                    JointHandle::Ring(axis) => (
                        TrackKind::Rotation,
                        gizmo.turned(axis, gizmo.drag_angle(axis, start, uv(p), aspect)),
                    ),
                    JointHandle::Arrow(axis) => {
                        let d = [uv(p)[0] - start[0], uv(p)[1] - start[1]];
                        let t = gizmo.moved(axis, gizmo.drag_distance(axis, d, aspect));
                        (TrackKind::Translation, [t[0], t[1], t[2], 0.])
                    }
                };
                self.joint_drag = Some((handle, start, value));
                actions.animation_preview = Some(Some(ChannelValue {
                    node: gizmo.node,
                    kind,
                    value,
                }));
            }
            if response.drag_stopped()
                && let Some((h, _, value)) = self.joint_drag.take()
                && h == handle
            {
                let kind = match handle {
                    JointHandle::Ring(_) => TrackKind::Rotation,
                    JointHandle::Arrow(_) => TrackKind::Translation,
                };
                actions.animation_key = Some(AnimationKey {
                    clip: gizmo.clip,
                    time: gizmo.time,
                    channel: ChannelValue {
                        node: gizmo.node,
                        kind,
                        value,
                    },
                    remove: false,
                });
            }
            response.on_hover_text(match handle {
                JointHandle::Ring(0) => "Тяните по красному кольцу — поворот сустава вокруг X; отпускание — ключ в текущем времени",
                JointHandle::Ring(1) => "Тяните по зелёному кольцу — поворот вокруг Y; отпускание — ключ",
                JointHandle::Ring(_) => "Тяните по синему кольцу — поворот вокруг Z; отпускание — ключ",
                JointHandle::Arrow(_) => "Тяните — сдвинуть сустав вдоль оси; отпускание — ключ в текущем времени",
            });
        }
    }
}
