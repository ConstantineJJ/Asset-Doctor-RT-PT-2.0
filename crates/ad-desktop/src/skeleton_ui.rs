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
            let side = egui::vec2(-dir.y, dir.x) * 4.5;
            let shape = vec![a + side, b, a - side];
            painter.add(egui::Shape::convex_polygon(
                shape.clone(),
                color.gamma_multiply(0.55),
                Stroke::new(1., OUTLINE),
            ));
            painter.line_segment([a, b], Stroke::new(2., color));
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
            .fixed_pos(p - egui::vec2(11., 11.))
            .show(context, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(22., 22.), egui::Sense::click());
                let is_selected = Some(joint.node) == selected;
                let radius = if is_selected {
                    8.
                } else if response.hovered() {
                    7.
                } else {
                    5.5
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

impl ViewerUi {
    /// ANIM-05: the root path of the clip (blue), its start (green) and end
    /// (red), and where the root is now (white ring).
    pub(super) fn root_path_ui(
        &self,
        root: &egui::Ui,
        path: &(Vec<Option<[f32; 2]>>, Option<[f32; 2]>),
    ) {
        let scene = self.scene;
        let at = |p: [f32; 2]| {
            egui::pos2(
                scene.min.x + p[0] * scene.width(),
                scene.min.y + p[1] * scene.height(),
            )
        };
        let painter = root.painter().with_clip_rect(scene);
        let blue = crate::accent::Accent::Existing.color();
        let (points, now) = path;
        for pair in points.windows(2) {
            if let [Some(a), Some(b)] = pair {
                painter.line_segment([at(*a), at(*b)], Stroke::new(3.5, OUTLINE));
                painter.line_segment([at(*a), at(*b)], Stroke::new(2., blue));
            }
        }
        if let Some(start) = points.first().copied().flatten() {
            painter.circle(
                at(start),
                5.,
                Color32::from_rgb(110, 214, 120),
                Stroke::new(1.5, OUTLINE),
            );
        }
        if let Some(end) = points.last().copied().flatten() {
            painter.circle(
                at(end),
                5.,
                Color32::from_rgb(236, 84, 84),
                Stroke::new(1.5, OUTLINE),
            );
        }
        if let Some(p) = now {
            painter.circle(at(*p), 6.5, blue, Stroke::new(2., Color32::WHITE));
        }
    }
}

impl ViewerUi {
    /// RIG-05: a line from each joint to its suggested position; selected
    /// suggestions in green, the others grey.
    pub(super) fn fit_ui(&self, root: &egui::Ui) {
        let scene = self.scene;
        let at = |p: [f32; 2]| {
            egui::pos2(
                scene.min.x + p[0] * scene.width(),
                scene.min.y + p[1] * scene.height(),
            )
        };
        let painter = root.painter().with_clip_rect(scene);
        let green = Color32::from_rgb(110, 228, 150);
        let grey = Color32::from_rgb(150, 156, 170);
        for (now, suggested, selected) in &self.fit_uv {
            let Some(target) = suggested.map(at) else {
                continue;
            };
            let color = if *selected { green } else { grey };
            if let Some(from) = now.map(at) {
                painter.line_segment([from, target], Stroke::new(3.5, OUTLINE));
                painter.line_segment([from, target], Stroke::new(1.8, color));
            }
            let r = if *selected { 6. } else { 4. };
            painter.circle(target, r, color, Stroke::new(1.5, OUTLINE));
        }
    }
}

/// A part of the joint gizmo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JointHandle {
    Ring(usize),
    Arrow(usize),
    /// Move in two axes (`gizmo_paint::PLANES`).
    Plane(usize),
    /// RIG-02: twist about the joint's own bone.
    Twist,
}

/// The twist ring's colour: a warm yellow, apart from the axis colours.
const TWIST_COLOR: Color32 = Color32::from_rgb(250, 210, 80);

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
    /// Rings turn the joint (by the pointer's angle around the centre, with
    /// the angle shown), arrows move it along an axis and plane squares
    /// along two: preview while dragging, one key of the current clip on
    /// release. One handle is hot at a time: the dragged one, else an arrow
    /// under the pointer, a plane square, then the nearest ring.
    pub(super) fn joint_gizmo_ui(
        &mut self,
        context: &egui::Context,
        root: &egui::Ui,
        gizmo: &crate::joint_gizmo::JointGizmo,
        actions: &mut Actions,
    ) {
        use crate::gizmo_paint as gp;
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
        let twist_ring: Vec<Option<egui::Pos2>> =
            gizmo.twist.as_ref().map_or_else(Vec::new, |(_, ring, _)| {
                ring.iter().map(|p| p.map(at)).collect()
            });
        let rings = gizmo
            .rings
            .clone()
            .map(|ring| ring.into_iter().map(|p| p.map(at)).collect::<Vec<_>>());
        let tips = gizmo.arrows.map(|t| t.map(at));
        let planes = gizmo.planes.map(|q| {
            let [a, b, c, d] = q;
            Some([at(a?), at(b?), at(c?), at(d?)])
        });
        let arrow_grab = |axis: usize| {
            tips[axis].map(|tip| {
                if (tip - centre).length() < 6. {
                    tip
                } else {
                    tip + (tip - centre).normalized() * 8.
                }
            })
        };
        let ring_distance = |axis: usize, p: egui::Pos2| {
            let ring = &rings[axis];
            (0..ring.len())
                .filter_map(|i| match (ring[i], ring[(i + 1) % ring.len()]) {
                    (Some(a), Some(b)) => Some(segment_distance(p, a, b)),
                    _ => None,
                })
                .fold(f32::INFINITY, f32::min)
        };
        let hot = self.joint_drag.map(|d| d.0).or_else(|| {
            let p = pointer.filter(|p| scene.contains(*p))?;
            (0..3)
                .find(|&axis| arrow_grab(axis).is_some_and(|g| g.distance(p) < 14.))
                .map(JointHandle::Arrow)
                .or_else(|| {
                    (0..3)
                        .find(|&i| planes[i].is_some_and(|q| gp::in_quad(p, &q)))
                        .map(JointHandle::Plane)
                })
                .or_else(|| {
                    let distance = |ring: &[Option<egui::Pos2>]| {
                        (0..ring.len())
                            .filter_map(|i| match (ring[i], ring[(i + 1) % ring.len()]) {
                                (Some(a), Some(b)) => Some(segment_distance(p, a, b)),
                                _ => None,
                            })
                            .fold(f32::INFINITY, f32::min)
                    };
                    let score = |ring: &[Option<egui::Pos2>], d: f32| {
                        let points: Vec<_> = ring.iter().flatten().copied().collect();
                        gp::ring_score(d, gp::openness(&points))
                    };
                    (0..3)
                        .map(|axis| {
                            let d = ring_distance(axis, p);
                            (JointHandle::Ring(axis), d, score(&rings[axis], d))
                        })
                        .chain((!twist_ring.is_empty()).then(|| {
                            let d = distance(&twist_ring);
                            (JointHandle::Twist, d, score(&twist_ring, d))
                        }))
                        .filter(|(_, d, _)| *d < 9.)
                        .min_by(|a, b| a.2.total_cmp(&b.2))
                        .map(|(handle, _, _)| handle)
                })
        });
        for (i, quad) in planes.iter().enumerate() {
            if let Some(quad) = quad {
                gp::plane(
                    &painter,
                    quad,
                    AXIS_COLORS[gp::PLANE_NORMAL[i]],
                    hot == Some(JointHandle::Plane(i)),
                );
            }
        }
        for axis in 0..3 {
            let ring_hot = hot == Some(JointHandle::Ring(axis));
            let ring = &rings[axis];
            for i in 0..ring.len() {
                if let (Some(a), Some(b)) = (ring[i], ring[(i + 1) % ring.len()]) {
                    painter.line_segment(
                        [a, b],
                        egui::Stroke::new(if ring_hot { 4. } else { 2.4 }, AXIS_COLORS[axis]),
                    );
                }
            }
        }
        {
            let hot_twist = hot == Some(JointHandle::Twist);
            for i in 0..twist_ring.len() {
                if let (Some(a), Some(b)) = (twist_ring[i], twist_ring[(i + 1) % twist_ring.len()])
                {
                    painter.line_segment(
                        [a, b],
                        egui::Stroke::new(if hot_twist { 4. } else { 2.4 }, TWIST_COLOR),
                    );
                }
            }
        }
        for axis in 0..3 {
            if let Some(tip) = tips[axis] {
                let arrow_hot = hot == Some(JointHandle::Arrow(axis));
                painter.line_segment([centre, tip], egui::Stroke::new(3., AXIS_COLORS[axis]));
                gp::arrow_head(
                    &painter,
                    centre,
                    tip,
                    if arrow_hot { 10. } else { 8. },
                    AXIS_COLORS[axis],
                );
            }
        }
        painter.circle_filled(centre, 4., Color32::WHITE);
        self.pointer_on_gizmo |= hot.is_some();
        // One grab area for the whole gizmo, shown every frame: a new area
        // is invisible in its first frame and would miss a quick press.
        let grab = hot.zip(pointer).map(|(handle, p)| {
            // Arrows are grabbed at their head; planes and rings anywhere.
            let position = match handle {
                JointHandle::Arrow(axis) if self.joint_drag.is_none() => {
                    arrow_grab(axis).unwrap_or(p)
                }
                _ => p,
            };
            (handle, position)
        });
        let area = egui::Area::new(egui::Id::new(("joint_gizmo_grab", gizmo.document)))
            .order(egui::Order::Middle)
            .fixed_pos(grab.map_or(centre, |g| g.1) - egui::vec2(13., 13.))
            .show(context, |ui| {
                grab.map(|(handle, position)| {
                    let id = egui::Id::new(("joint_gizmo", gizmo.document, format!("{handle:?}")));
                    let rect = egui::Rect::from_center_size(position, egui::vec2(26., 26.));
                    ui.interact(rect, id, egui::Sense::drag())
                })
            });
        if let (Some((handle, _)), Some(response)) = (grab, area.inner) {
            if response.drag_started_by(egui::PointerButton::Primary)
                && let Some(p) = response.interact_pointer_pos()
            {
                let base = match handle {
                    JointHandle::Ring(_) | JointHandle::Twist => gizmo.rotation,
                    JointHandle::Arrow(_) | JointHandle::Plane(_) => {
                        let t = gizmo.translation;
                        [t[0], t[1], t[2], 0.]
                    }
                };
                self.joint_drag = Some((handle, uv(p), base));
                self.joint_turn = gp::TurnTrack::new(centre, p);
            }
            if response.dragged_by(egui::PointerButton::Primary)
                && let (Some(p), Some((h, start, _))) =
                    (response.interact_pointer_pos(), self.joint_drag)
                && h == handle
            {
                let d = [uv(p)[0] - start[0], uv(p)[1] - start[1]];
                let update = match handle {
                    JointHandle::Ring(axis) => {
                        // Grabbed inside the dead zone: the turn starts once
                        // the pointer leaves it.
                        if self.joint_turn.is_none() {
                            self.joint_turn = gp::TurnTrack::new(centre, p);
                        }
                        self.joint_turn.as_mut().map(|track| {
                            track.step(centre, p);
                            (
                                TrackKind::Rotation,
                                gizmo.turned(axis, track.swept * gizmo.away[axis]),
                            )
                        })
                    }
                    JointHandle::Twist => {
                        if self.joint_turn.is_none() {
                            self.joint_turn = gp::TurnTrack::new(centre, p);
                        }
                        match (self.joint_turn.as_mut(), gizmo.twist.as_ref()) {
                            (Some(track), Some((axis, _, away))) => {
                                track.step(centre, p);
                                Some((
                                    TrackKind::Rotation,
                                    gizmo.turned_about(
                                        glam::Vec3::from_array(*axis),
                                        track.swept * away,
                                    ),
                                ))
                            }
                            _ => None,
                        }
                    }
                    JointHandle::Arrow(axis) => {
                        let t = gizmo.moved(axis, gizmo.drag_distance(axis, d, aspect));
                        Some((TrackKind::Translation, [t[0], t[1], t[2], 0.]))
                    }
                    JointHandle::Plane(plane) => {
                        gizmo.drag_plane(plane, d, aspect).map(|(a, b)| {
                            let t = gizmo.moved_in_plane(plane, a, b);
                            (TrackKind::Translation, [t[0], t[1], t[2], 0.])
                        })
                    }
                };
                if let Some((kind, value)) = update {
                    self.joint_drag = Some((handle, start, value));
                    actions.animation_preview = Some(Some(ChannelValue {
                        node: gizmo.node,
                        kind,
                        value,
                    }));
                }
            }
            if response.drag_stopped_by(egui::PointerButton::Primary)
                && let Some((h, _, value)) = self.joint_drag.take()
                && h == handle
            {
                self.joint_turn = None;
                let kind = match handle {
                    JointHandle::Ring(_) | JointHandle::Twist => TrackKind::Rotation,
                    JointHandle::Arrow(_) | JointHandle::Plane(_) => TrackKind::Translation,
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
                JointHandle::Ring(0) => "Тяните по красному кольцу — поворот сустава вокруг X. Чем дальше мышь от центра, тем точнее; угол виден у курсора. Отпускание — ключ в текущем времени",
                JointHandle::Ring(1) => "Тяните по зелёному кольцу — поворот вокруг Y; чем дальше мышь, тем точнее. Отпускание — ключ",
                JointHandle::Ring(_) => "Тяните по синему кольцу — поворот вокруг Z; чем дальше мышь, тем точнее. Отпускание — ключ",
                JointHandle::Arrow(_) => "Тяните — сдвинуть сустав вдоль оси; отпускание — ключ в текущем времени",
                JointHandle::Plane(_) => "Тяните квадрат — сдвинуть сустав в плоскости двух осей; отпускание — ключ в текущем времени",
                JointHandle::Twist => "Тяните по жёлтому кольцу — скрутить сустав вокруг его кости; чем дальше мышь, тем точнее. Отпускание — ключ в текущем времени",
            });
        }
        if let (Some((handle, _, _)), Some(track), Some(p)) =
            (self.joint_drag, self.joint_turn, pointer)
        {
            let shown = match handle {
                JointHandle::Ring(axis) => Some((gizmo.away[axis], AXIS_COLORS[axis])),
                JointHandle::Twist => gizmo
                    .twist
                    .as_ref()
                    .map(|(_, _, away)| (*away, TWIST_COLOR)),
                _ => None,
            };
            if let Some((away, color)) = shown {
                let degrees = (track.swept * away).to_degrees();
                gp::turn_indicator(&painter, centre, p, &track, degrees, color);
            }
        }
    }
}
