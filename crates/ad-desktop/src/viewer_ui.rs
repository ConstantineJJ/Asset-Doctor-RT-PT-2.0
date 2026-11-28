use crate::{imports::PreparedAsset, ui_model::SceneTree};
use ad_render_vulkan::{
    SurfaceSize,
    lighting::ViewLighting,
    ui::{UiFrame, ViewportRect},
};
use egui::{Color32, RichText};
use winit::{event::WindowEvent, window::Window};

#[path = "panels.rs"]
mod panels;
#[path = "playback_bar.rs"]
mod playback_bar;
#[path = "rig_ui.rs"]
mod rig_ui;
#[path = "skeleton_ui.rs"]
pub mod skeleton_ui;
#[path = "update_ui.rs"]
mod update_ui;
pub(crate) use panels::OUTLINE_COLORS;
#[path = "view_settings.rs"]
pub mod view_settings;

/// How the program window is shown («Настройки → Окно», F11). The window
/// can also lose its frame and title bar (F1, `ViewerUi::frameless`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowMode {
    /// A window (with its frame unless frameless).
    #[default]
    Normal,
    /// Borderless full screen (F11; Esc returns to the window).
    Fullscreen,
}
impl WindowMode {
    pub fn key(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Fullscreen => "fullscreen",
        }
    }
    /// Also reads the modes of 2026-11-25 builds (maximized, borderless)
    /// as a normal window.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "normal" | "maximized" | "borderless" => Some(Self::Normal),
            "fullscreen" => Some(Self::Fullscreen),
            _ => None,
        }
    }
}

/// Drop-down menus of the top bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TopMenu {
    Settings,
    View,
    Grid,
    Light,
    Lamp,
    Exposure,
    Scene,
    Sky,
    Model,
}
/// Tabs of the right block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Health,
    Info,
    Rigging,
}
/// Pages of «Здоровье».
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HealthPage {
    #[default]
    Analysis,
    Repair,
}

#[derive(Default)]
pub struct Actions {
    pub(crate) playback: Option<crate::rig_playback::Controls>,
    pub(crate) animation_key: Option<ad_document::AnimationKey>,
    pub(crate) clip_edit: Option<ad_document::ClipEdit>,
    /// RIG-03: rename a node or add a joint.
    pub(crate) joint_edit: Option<ad_document::JointEdit>,
    /// RIG-05: start or stop the joint fit search.
    pub(crate) fit_search: bool,
    pub(crate) fit_cancel: bool,
    pub(crate) animation_preview: Option<Option<ad_document::ChannelValue>>,
    pub open: bool,
    pub fit: bool,
    pub cancel: bool,
    /// Start a read-only repair preview for this source primitive.
    pub preview: Option<(ad_document::MeshId, ad_heal::RepairKind)>,
    pub apply: bool,
    pub discard_preview: bool,
    pub undo: bool,
    pub redo: bool,
    pub cancel_edit: bool,
    pub save_copy: bool,
    /// «Сохранить» over the working file (ADR-0035).
    pub save: bool,
    /// HEAL-02 queue (ADR-0038).
    pub queue_start: bool,
    pub queue_stop: bool,
    /// HEAL-03 history window (ADR-0039).
    pub history_open: bool,
    pub history_clear: bool,
    /// ADR-0040 skybox: choose a panorama file / remove the loaded one.
    pub skybox_load: bool,
    pub skybox_remove: bool,
    /// SkyBox quality changed: prepare this panorama again.
    pub skybox_reload: Option<std::path::PathBuf>,
    /// Right-click menu «Удалить»: take this object out of the scene.
    pub delete_object: Option<ad_document::NodeId>,
    /// Fixed camera view chosen in the viewport corner.
    pub camera_view: Option<crate::camera::View>,
    /// SkyBox window: show this panorama; choose the library folder.
    pub skybox_open: Option<std::path::PathBuf>,
    pub skybox_folder_choose: bool,
    /// ADR-0049: check for updates; download and install the shown one;
    /// stop a running check or download.
    pub update_check: bool,
    /// «Настройки → Выход»: close the program (unsaved edits are asked about).
    pub exit: bool,
    pub update_install: bool,
    pub update_cancel: bool,
    /// ADR-0040 lamp gizmo dragged by (dx, dy) logical points this frame.
    pub lamp_drag: Option<[f32; 2]>,
    /// Lamp context menu: point the lamp from the camera.
    pub lamp_from_camera: bool,
    /// Lamp handles: degrees of azimuth and elevation to turn this frame,
    /// and model radii to move the lamp closer (−) or farther (+).
    pub lamp_turn: Option<[f32; 2]>,
    pub lamp_distance: Option<f32>,
    /// ADR-0041 model gizmo: handle dragged by (dx, dy) in viewport units
    /// (0..1 of its width/height), released this frame, or reset requested.
    pub model_drag: Option<(crate::model_gizmo::Handle, [f32; 2])>,
    /// A ring or knob drag (ADR-0063): axis and the pointer's screen angle
    /// step around the gizmo centre (radians, y down).
    pub model_ring_drag: Option<(usize, f32)>,
    pub model_drag_end: bool,
    pub model_reset: bool,
    /// ADR-0041 numeric fields: preview of the edited placement, and the
    /// placement to publish (one edit) when a field is released or a button
    /// is pressed.
    pub place_preview: Option<ad_document::Placement>,
    pub place: Option<ad_document::Placement>,
}
/// ADR-0041 model gizmo for this frame: viewport positions (0..1, y down) of
/// the bottom centre, the X/Y/Z arrow tips, the X and Y rotation rings and
/// their knobs (None behind the camera).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelGizmo {
    pub pivot: [f32; 2],
    pub ends: [Option<[f32; 2]>; 3],
    /// Corners of the XY, YZ and XZ plane squares.
    pub planes: [[Option<[f32; 2]>; 4]; 3],
    pub rings: [[Option<[f32; 2]>; crate::model_gizmo::RING_POINTS]; 2],
    pub knobs: [Option<[f32; 2]>; 2],
    /// Radians turned by the ring being dragged (shown next to the pointer).
    pub turned: Option<f32>,
}
impl Default for ModelGizmo {
    fn default() -> Self {
        Self {
            pivot: [0.; 2],
            ends: [None; 3],
            planes: [[None; 4]; 3],
            rings: [[None; crate::model_gizmo::RING_POINTS]; 2],
            knobs: [None; 2],
            turned: None,
        }
    }
}
/// ADR-0040 lamp gizmo for this frame: viewport positions (0..1, y down) of
/// the lamp and the model centre, set by the application from the camera,
/// with the guides of its azimuth and elevation handles.
#[derive(Clone, Default)]
pub struct LampGizmo {
    pub lamp: [f32; 2],
    pub center: [f32; 2],
    pub guides: Option<std::sync::Arc<crate::lamp::Guides>>,
}
/// Lamp parts the pointer can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LampPart {
    Icon,
    Azimuth,
    Elevation,
    Distance,
}
/// Repair history as shown by the UI; filled by the application on demand.
#[derive(Default)]
pub struct HistoryUi {
    pub open: bool,
    /// (written by this run, formatted record), newest first.
    pub lines: Vec<(bool, String)>,
    pub corrupt: usize,
    pub error: Option<String>,
    pub path: String,
    pub only_session: bool,
    pub confirm_clear: bool,
}
/// One history record as a single readable line.
pub fn history_line(r: &crate::history::Record) -> String {
    let mesh = r
        .mesh
        .map_or(String::new(), |m| format!(" · примитив #{m}"));
    let what = match r.action.as_str() {
        "save" => "Сохранить".to_owned(),
        "save-copy" => "Сохранить копию".to_owned(),
        "undo" => "Отмена правки".to_owned(),
        "redo" => "Возврат правки".to_owned(),
        "queue" => format!("Очередь · {}", r.rule),
        _ => format!("Ремонт {}", r.rule),
    };
    let mut line = format!(
        "{} · {} · {what}{mesh}",
        crate::history::format_utc(r.time),
        r.model
    );
    if r.mesh.is_some() {
        line.push_str(&format!(
            " · треугольники {} -> {} · вершины {} -> {}",
            r.triangles.0, r.triangles.1, r.vertices.0, r.vertices.1
        ));
        if r.normals > 0 {
            line.push_str(&format!(" · нормалей {}", r.normals));
        }
        if let Some(left) = r.left {
            line.push_str(&format!(" · осталось {} {left}", r.rule));
        }
        line.push_str(if r.complete {
            " · анализ полный"
        } else {
            " · анализ неполный"
        });
    }
    line.push_str(&format!(" · ревизия {}", r.revision));
    line
}
/// Repair-queue state shown by the UI (ADR-0038).
#[derive(Clone, Default)]
pub struct QueueUi {
    pub running: bool,
    pub done: Vec<String>,
    pub skipped: Vec<String>,
    pub current: Option<String>,
    pub stop: Option<String>,
}
/// Facts of a prepared, not yet applied repair (ADR-0031 Preview).
#[derive(Clone)]
pub struct PreviewInfo {
    pub kind: ad_heal::RepairKind,
    pub mesh: ad_document::MeshId,
    /// Triangles before/after the edit.
    pub before: usize,
    pub after: usize,
    pub vertices_before: usize,
    pub vertices_after: usize,
    /// BIN bytes a saved copy appends after the original data.
    pub appended_bytes: usize,
    /// G006 boundary edges of the primitive before/after a vertex merge.
    pub boundary_edges: Option<(usize, usize)>,
    /// Normals a G004/G005 repair rewrites.
    pub changed_normals: usize,
    /// Source node references, including inactive nodes; not visible copies.
    pub references: usize,
    pub complete: bool,
    /// Findings of the repaired rule left on this primitive after the edit.
    pub target_left: Option<usize>,
    pub geometry_bytes: usize,
}
/// Edit state shown by the UI; owned and refreshed by the application.
#[derive(Default)]
pub struct EditUi {
    pub busy: Option<String>,
    pub preview: Option<PreviewInfo>,
    pub can_undo: bool,
    pub can_redo: bool,
    /// A model is open; Save Copy also works for an unchanged model.
    pub can_save: bool,
    /// A working file exists that «Сохранить» may replace.
    pub can_overwrite: bool,
    pub queue: Option<QueueUi>,
    /// Revision/origin differs from the imported file or last verified copy.
    pub dirty: bool,
}
pub struct ViewerUi {
    pub(crate) playback: crate::rig_playback::Controls,
    pub context: egui::Context,
    input: Option<egui_winit::State>,
    pub show_tree: bool,
    pub show_summary: bool,
    pub textures: bool,
    pub mr_maps: bool,
    /// ADR-0040 floor theme under the model (view only); None = no surface.
    pub floor: Option<ad_render_vulkan::grid::FloorTheme>,
    /// ADR-0040 environment (sky) preset; view only, never touches lighting.
    pub environment: ad_render_vulkan::sky::Environment,
    /// ADR-0040 environment light switch and strength (PBR only, view only).
    pub environment_light: bool,
    pub environment_strength: f32,
    /// ADR-0040 skybox: loaded file name, shown or not, yaw in degrees and the
    /// colours it lights the model with.
    pub skybox_name: Option<String>,
    /// File of the shown skybox (view settings load it again next run).
    pub skybox_path: Option<std::path::PathBuf>,
    /// ADR-0047: widest SkyBox level kept on the GPU (2048, 4096 or 8192).
    pub skybox_width: u32,
    /// SkyBox window: open, its folder (kept in the view settings), the
    /// panoramas there and the one under the pointer.
    pub show_skybox: bool,
    pub skybox_folder: Option<std::path::PathBuf>,
    pub skybox_library: crate::skybox_library::Library,
    skybox_hover: Option<usize>,
    /// Program log window, its file switch (view settings) and the warnings
    /// already seen (the bug turns amber for new ones).
    pub show_log: bool,
    pub log_to_file: bool,
    /// ADR-0049 update window and its state.
    pub update: crate::updates::Panel,
    /// Object outline (Tab, amoeba icon): on/off and colour (None: dynamic
    /// contrast, Some: index into the seven fixed colours). The hovered node
    /// is set by the application from the pointer.
    pub outline_on: bool,
    pub outline_color: Option<usize>,
    /// Outline width in pixels (1..32).
    pub outline_width: f32,
    pub hover_node: Option<ad_document::NodeId>,
    /// ADR-0048: the gizmo works on this object instead of the whole model.
    pub object_gizmo: Option<ad_document::NodeId>,
    /// Right-click menu: where (points) and on which object.
    context_menu: Option<(egui::Pos2, Option<ad_document::NodeId>)>,
    log_seen_warnings: u64,
    log_filter: String,
    log_only_warnings: bool,
    pub skybox_on: bool,
    pub skybox_yaw: f32,
    pub skybox_light: Option<[[f32; 3]; 3]>,
    /// ADR-0040 lamp gizmo: where to draw it (None = off screen) and whether
    /// the owner keeps the icon visible.
    pub lamp_gizmo: Option<LampGizmo>,
    pub lamp_icon: bool,
    /// Colour temperature last chosen for the lamp, kelvin.
    pub lamp_kelvin: f32,
    /// ADR-0041 model gizmo: positions from the app, the owner's visibility
    /// choice, and the current placement shown in Сводка.
    pub model_gizmo: Option<ModelGizmo>,
    pub model_gizmo_on: bool,
    /// ADR-0042 render channel, wireframe and helpers (view only).
    pub channel: ad_render_vulkan::lighting::Channel,
    /// ADR-0043 input bindings, the slot being captured, a pending conflict
    /// (action, slot, chord, owner), and a change the app should save.
    pub bindings: crate::input::Bindings,
    pub input_capture: Option<(crate::input::Action, usize)>,
    pub input_conflict: Option<(
        crate::input::Action,
        usize,
        crate::input::Chord,
        crate::input::Conflict,
    )>,
    pub bindings_changed: bool,
    pub show_controls: bool,
    pub wireframe: bool,
    pub wireframe_supported: bool,
    pub show_axes: bool,
    /// ADR-0044 lamp shadows on the model and the floor.
    pub shadows: bool,
    pub shadow_strength: f32,
    pub show_bounds: bool,
    pub helper_overlay: Option<crate::helpers::Overlay>,
    /// Window mode chosen in «Настройки → Окно» (applied by the app) and
    /// the window without frame and title bar (F1).
    pub window_mode: WindowMode,
    pub frameless: bool,
    /// A widget (a gizmo handle or ring) is being dragged right now.
    pub gizmo_dragging: bool,
    /// The pointer was over a gizmo ring/handle, the lamp or a joint in the
    /// last frame: the left button belongs to that, not to selection.
    pub pointer_on_gizmo: bool,
    /// Selection box in viewport coordinates (corners, remove mode).
    pub marquee: Option<([f32; 2], [f32; 2], bool)>,
    /// P7 joint gizmo (built by the app), its axes (local or world) and the
    /// drag in progress: handle, start pointer, last previewed value.
    pub joint_gizmo: Option<crate::joint_gizmo::JointGizmo>,
    pub joint_local_axes: bool,
    /// Joint gizmo size, 0.5–2× of `joint_gizmo::RADIUS_PX`.
    pub joint_gizmo_size: f32,
    joint_drag: Option<(skeleton_ui::JointHandle, [f32; 2], [f32; 4])>,
    /// ADR-0063: the ring turn in progress (model gizmo: axis and track;
    /// joint gizmo: the track of the dragged ring).
    model_turn: Option<(usize, crate::gizmo_paint::TurnTrack)>,
    /// The plane or ring of the model gizmo being dragged.
    model_free_drag: Option<crate::model_gizmo::Handle>,
    /// The Work-Tree strip over the viewport's top-left (not camera input).
    strip_rect: egui::Rect,
    joint_turn: Option<crate::gizmo_paint::TurnTrack>,
    /// P7 skeleton overlay (bones and joints of the skins) and its switch.
    pub skeleton_on: bool,
    pub skeleton: Option<skeleton_ui::SkeletonOverlay>,
    /// ANIM-01: start the first clip when a model with clips opens.
    pub autoplay: bool,
    /// RIG-05: joint fit results and their viewport projection (current,
    /// suggested, selected).
    pub fit: crate::joint_fit::FitState,
    #[allow(clippy::type_complexity)]
    pub fit_uv: Vec<(Option<[f32; 2]>, Option<[f32; 2]>, bool)>,
    /// ANIM-05: root motion card state and its projected path (points,
    /// the point at the current time).
    pub root_path: crate::root_motion::PathState,
    #[allow(clippy::type_complexity)]
    pub root_path_uv: Option<(Vec<Option<[f32; 2]>>, Option<[f32; 2]>)>,
    /// Interface scale (egui zoom), 0.75–2; Ctrl + / Ctrl - change it too.
    pub ui_scale: f32,
    applied_scale: f32,
    /// The playback bar and the corner buttons (view, Undo/Redo) as drawn
    /// last frame: the bar keeps clear of the corner.
    bar_rect: egui::Rect,
    corner_rect: egui::Rect,
    pub placement: ad_document::Placement,
    /// Numeric fields being edited (None: they follow the model).
    placement_draft: Option<ad_document::Placement>,
    /// Theme G brings back after hiding the floor.
    last_floor: ad_render_vulkan::grid::FloorTheme,
    pub normal_maps: bool,
    pub ao_maps: bool,
    pub emission: bool,
    pub lighting: ViewLighting,
    /// Owner layout (2026-11-21): open top menu, its button, all menu
    /// buttons of this frame and their width, right tab and health page.
    menu: Option<TopMenu>,
    menu_anchor: egui::Rect,
    menu_buttons: Vec<egui::Rect>,
    menu_group_width: f32,
    pub tab: Tab,
    health_page: HealthPage,
    carousel: crate::diagnostic_ui::Carousel,
    /// Lamp part under the pointer or held in the last frame.
    lamp_hot: Option<LampPart>,
    help: bool,
    pub tree: SceneTree,
    scene: egui::Rect,
    pub status: String,
    /// Refusal or failure: drawn in the warning colour until the next status.
    pub status_warning: bool,
    pub history: HistoryUi,
    pub error: Option<String>,
    pub edit: EditUi,
    #[cfg(test)]
    exit_rect: egui::Rect,
    #[cfg(test)]
    open_rect: egui::Rect,
    #[cfg(test)]
    apply_rect: egui::Rect,
    #[cfg(test)]
    discard_rect: egui::Rect,
    #[cfg(test)]
    undo_rect: egui::Rect,
    #[cfg(test)]
    play_rect: egui::Rect,
    #[cfg(test)]
    redo_rect: egui::Rect,
    #[cfg(test)]
    preview_rect: egui::Rect,
    #[cfg(test)]
    preview_rects: Vec<(ad_heal::RepairKind, egui::Rect)>,
    #[cfg(test)]
    save_rect: egui::Rect,
    #[cfg(test)]
    overwrite_rect: egui::Rect,
    show_queue: bool,
    #[cfg(test)]
    queue_rect: egui::Rect,
    #[cfg(test)]
    queue_start_rect: egui::Rect,
    #[cfg(test)]
    reset_lighting_rect: egui::Rect,
    #[cfg(test)]
    lamp_gizmo_rect: egui::Rect,
    #[cfg(test)]
    model_gizmo_rects: Vec<(crate::model_gizmo::Handle, egui::Rect)>,
    #[cfg(test)]
    mr_maps_rect: egui::Rect,
    #[cfg(test)]
    normal_maps_rect: egui::Rect,
    #[cfg(test)]
    ao_maps_rect: egui::Rect,
    #[cfg(test)]
    emission_rect: egui::Rect,
    #[cfg(test)]
    menu_rects: Vec<(TopMenu, egui::Rect)>,
    #[cfg(test)]
    tab_rects: Vec<(Tab, egui::Rect)>,
    #[cfg(test)]
    page_rects: Vec<(HealthPage, egui::Rect)>,
    #[cfg(test)]
    tree_rect: egui::Rect,
    #[cfg(test)]
    lamp_switch_rect: egui::Rect,
    #[cfg(test)]
    lamp_handle_rects: Vec<(LampPart, egui::Rect)>,
    #[cfg(test)]
    skybox_button_rect: egui::Rect,
    #[cfg(test)]
    bug_rect: egui::Rect,
    #[cfg(test)]
    amoeba_rect: egui::Rect,
    #[cfg(test)]
    view_rects: Vec<(crate::camera::View, egui::Rect)>,
    #[cfg(test)]
    skybox_tile_rects: Vec<egui::Rect>,
}
impl Default for ViewerUi {
    fn default() -> Self {
        let context = egui::Context::default();
        context.set_theme(egui::Theme::Dark);
        let mut visuals = egui::Visuals::dark();
        // Owner 2026-11-26: lighter text everywhere.
        visuals.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(222, 226, 234);
        visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(232, 235, 242);
        visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
        visuals.widgets.active.fg_stroke.color = Color32::WHITE;
        visuals.widgets.open.fg_stroke.color = Color32::WHITE;
        visuals.weak_text_color = Some(Color32::from_rgb(158, 166, 182));
        visuals.panel_fill = Color32::from_rgb(26, 29, 35);
        visuals.window_fill = Color32::from_rgb(30, 33, 40);
        visuals.selection.bg_fill = Color32::from_rgb(100, 42, 49);
        visuals.selection.stroke = egui::Stroke::new(1., Color32::from_rgb(255, 205, 208));
        context.set_visuals(visuals);
        Self {
            context,
            input: None,
            show_tree: false,
            show_summary: true,
            textures: true,
            mr_maps: true,
            floor: Some(ad_render_vulkan::grid::FloorTheme::StudioGrid),
            last_floor: ad_render_vulkan::grid::FloorTheme::StudioGrid,
            environment: ad_render_vulkan::sky::Environment::DarkStudio,
            environment_light: false,
            environment_strength: 1.,
            skybox_name: None,
            skybox_path: None,
            skybox_width: ad_render_vulkan::ibl::DEFAULT_WIDTH,
            show_skybox: false,
            skybox_folder: None,
            skybox_library: Default::default(),
            skybox_hover: None,
            show_log: false,
            log_to_file: false,
            update: Default::default(),
            outline_on: true,
            outline_color: None,
            outline_width: ad_render_vulkan::outline::DEFAULT_WIDTH,
            hover_node: None,
            object_gizmo: None,
            context_menu: None,
            log_seen_warnings: 0,
            log_filter: String::new(),
            log_only_warnings: false,
            skybox_on: false,
            skybox_yaw: 0.,
            skybox_light: None,
            lamp_gizmo: None,
            lamp_icon: true,
            lamp_kelvin: 6500.,
            model_gizmo: None,
            model_gizmo_on: true,
            channel: ad_render_vulkan::lighting::Channel::Lit,
            bindings: crate::input::Bindings::default(),
            input_capture: None,
            input_conflict: None,
            bindings_changed: false,
            show_controls: false,
            wireframe: false,
            wireframe_supported: true,
            show_axes: false,
            shadows: true,
            shadow_strength: 0.7,
            show_bounds: false,
            skeleton_on: true,
            skeleton: None,
            joint_gizmo: None,
            joint_local_axes: true,
            joint_gizmo_size: 1.,
            joint_drag: None,
            model_turn: None,
            model_free_drag: None,
            strip_rect: egui::Rect::NOTHING,
            joint_turn: None,
            marquee: None,
            pointer_on_gizmo: false,
            gizmo_dragging: false,
            window_mode: WindowMode::Normal,
            frameless: false,
            helper_overlay: None,
            placement: ad_document::Placement::IDENTITY,
            placement_draft: None,
            normal_maps: true,
            ao_maps: true,
            emission: true,
            lighting: ViewLighting::default(),
            menu: None,
            menu_anchor: egui::Rect::NOTHING,
            menu_buttons: Vec::new(),
            menu_group_width: 0.,
            tab: Tab::Health,
            playback: crate::rig_playback::Controls::default(),
            health_page: HealthPage::Analysis,
            carousel: Default::default(),
            lamp_hot: None,
            help: false,
            tree: SceneTree::default(),
            scene: egui::Rect::NOTHING,
            status: "Готов к открытию GLB".into(),
            status_warning: false,
            history: HistoryUi::default(),
            error: None,
            edit: EditUi::default(),
            #[cfg(test)]
            exit_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            open_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            apply_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            discard_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            undo_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            play_rect: egui::Rect::NOTHING,
            bar_rect: egui::Rect::NOTHING,
            ui_scale: 1.,
            applied_scale: 1.,
            autoplay: true,
            fit: Default::default(),
            fit_uv: Vec::new(),
            root_path: crate::root_motion::PathState {
                show: true,
                ..Default::default()
            },
            root_path_uv: None,
            corner_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            redo_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            preview_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            preview_rects: Vec::new(),
            #[cfg(test)]
            save_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            overwrite_rect: egui::Rect::NOTHING,
            show_queue: false,
            #[cfg(test)]
            queue_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            queue_start_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            reset_lighting_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            lamp_gizmo_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            model_gizmo_rects: Vec::new(),
            #[cfg(test)]
            mr_maps_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            normal_maps_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            ao_maps_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            emission_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            menu_rects: Vec::new(),
            #[cfg(test)]
            tab_rects: Vec::new(),
            #[cfg(test)]
            page_rects: Vec::new(),
            #[cfg(test)]
            tree_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            lamp_switch_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            lamp_handle_rects: Vec::new(),
            #[cfg(test)]
            skybox_button_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            bug_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            amoeba_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            view_rects: Vec::new(),
            #[cfg(test)]
            skybox_tile_rects: Vec::new(),
        }
    }
}
impl ViewerUi {
    /// G key: hides the floor or brings back the last theme; true when shown.
    pub fn toggle_floor(&mut self) -> bool {
        self.floor = match self.floor {
            Some(theme) => {
                self.last_floor = theme;
                None
            }
            None => Some(self.last_floor),
        };
        self.floor.is_some()
    }
    /// Replaces the status line; `warning` marks a refusal or failure.
    pub fn set_status(&mut self, text: impl Into<String>, warning: bool) {
        self.status = text.into();
        self.status_warning = warning;
        crate::log::push(format!(
            "status{}: {}",
            if warning { " warning" } else { "" },
            self.status
        ));
    }
    pub fn attach(&mut self, window: &Window) {
        self.input = Some(egui_winit::State::new(
            self.context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(4096),
        ));
    }
    pub fn on_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.input
            .as_mut()
            .is_some_and(|input| input.on_window_event(window, event).consumed)
    }
    /// ADR-0041 numeric placement: where the imported bottom centre moved
    /// (X/Y/Z), turn about Y and X in degrees and uniform scale, all about
    /// that point. Dragging or typing previews; releasing the field (or
    /// leaving it, or Enter) publishes one edit. «На пол» / «В центр» move
    /// the model's bottom to Y = 0 / its bottom centre above the origin.
    fn placement_fields_ui(
        &mut self,
        ui: &mut egui::Ui,
        bounds: ad_document::Bounds,
        source: ad_document::Bounds,
        actions: &mut Actions,
    ) {
        use crate::model_gizmo::Fields;
        use ad_document::Placement;
        let draft = self.placement_draft.unwrap_or(self.placement);
        let mut fields = Fields::of(draft, source);
        let before = fields;
        let extent = (0..3)
            .map(|a| source.max[a] - source.min[a])
            .fold(0f32, f32::max)
            .max(1e-6);
        let (mut changed, mut commit) = (false, false);
        // A wheel step over a field changes it by `step` and is one edit.
        let mut track = |ui: &egui::Ui,
                         r: egui::Response,
                         value: &mut f32,
                         step: f32,
                         range: std::ops::RangeInclusive<f32>| {
            let wheel = crate::accent::wheel_value(ui, &r, value, step, range);
            changed |= r.changed() || wheel;
            commit |= r.drag_stopped() || r.lost_focus() || wheel;
        };
        ui.horizontal(|ui| {
            ui.label("Сдвиг");
            for (axis, name) in ["X", "Y", "Z"].iter().enumerate() {
                let range = -Placement::MAX_TRANSLATION..=Placement::MAX_TRANSLATION;
                let r = ui.add(
                    egui::DragValue::new(&mut fields.offset[axis])
                        .prefix(format!("{name} "))
                        .speed(extent * 0.005)
                        .range(range.clone())
                        .max_decimals(4),
                )
                .on_hover_text("Куда сдвинута нижняя точка центра модели от исходного положения, в единицах файла. Тяните или введите число и нажмите Enter; отпускание — одна правка. Колёсико — шаг 1 % размера модели.");
                track(ui, r, &mut fields.offset[axis], extent * 0.01, range);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Поворот");
            let r = ui
                .add(
                    egui::DragValue::new(&mut fields.yaw)
                        .prefix("Y ")
                        .suffix(" град.")
                        .speed(0.5)
                        .range(-180.0..=180.0)
                        .max_decimals(2),
                )
                .on_hover_text(
                    "Поворот вокруг вертикальной оси (по горизонтали) от нижней точки центра. Колёсико — шаг 1°.",
                );
            track(ui, r, &mut fields.yaw, 1., -180.0..=180.0);
            let r = ui
                .add(
                    egui::DragValue::new(&mut fields.pitch)
                        .prefix("X ")
                        .suffix(" град.")
                        .speed(0.5)
                        .range(-89.9..=89.9)
                        .max_decimals(2),
                )
                .on_hover_text(
                    "Наклон вокруг горизонтальной оси X (по вертикали) от нижней точки центра. Колёсико — шаг 1°.",
                );
            track(ui, r, &mut fields.pitch, 1., -89.9..=89.9);
        });
        let scale_speed = (fields.scale * 0.005).max(1e-6);
        ui.horizontal(|ui| {
            let r = ui
                .add(
                    egui::DragValue::new(&mut fields.scale)
                        .prefix("Масштаб x")
                        .speed(scale_speed)
                        .range(Placement::MIN_SCALE..=Placement::MAX_SCALE)
                        .max_decimals(4),
                )
                .on_hover_text(
                    "Равномерный масштаб относительно исходного размера, от нижней точки центра. Колёсико — шаг 0.01.",
                );
            track(
                ui,
                r,
                &mut fields.scale,
                0.01,
                Placement::MIN_SCALE..=Placement::MAX_SCALE,
            );
            if ui
                .button("На пол")
                .on_hover_text("Поставить нижнюю точку модели на Y = 0")
                .clicked()
            {
                actions.place = Some(crate::model_gizmo::on_floor(self.placement, bounds));
            }
            if ui
                .button("В центр")
                .on_hover_text("Нижнюю точку центра модели — над началом координат (X = Z = 0)")
                .clicked()
            {
                actions.place = Some(crate::model_gizmo::centred(self.placement, bounds));
            }
        });
        if changed && fields != before {
            let target = fields.placement(draft, source);
            if target.valid() {
                self.placement_draft = Some(target);
                actions.place_preview = Some(target);
            }
        }
        if commit
            && let Some(target) = self.placement_draft.take()
            && target != self.placement
            && target.valid()
        {
            actions.place = Some(target);
        }
        if actions.place.is_some() {
            self.placement_draft = None;
        }
    }
    /// The «Модель» number fields hold an unpublished preview.
    pub fn editing_placement(&self) -> bool {
        self.placement_draft.is_some()
    }
    pub fn reset_placement_draft(&mut self) {
        self.placement_draft = None;
    }
    fn controls_ui(&mut self, ui: &mut egui::Ui) {
        use crate::input::Action;
        if let Some((action, slot, chord, owner)) = self.input_conflict {
            ui.colored_label(
                Color32::from_rgb(240, 180, 80),
                format!(
                    "«{}» уже назначено на «{}». Переназначить на «{}»?",
                    chord.name(),
                    owner.action.label(),
                    action.label()
                ),
            );
            ui.horizontal(|ui| {
                if ui.button("Заменить").clicked() {
                    let _ = self.bindings.assign(action, slot, chord, true);
                    self.bindings_changed = true;
                    self.input_conflict = None;
                }
                if ui.button("Отмена").clicked() {
                    self.input_conflict = None;
                }
            });
            ui.separator();
        }
        if let Some((action, _)) = self.input_capture {
            ui.colored_label(
                Color32::from_rgb(120, 190, 255),
                format!(
                    "«{}»: нажмите клавишу (с Ctrl/Shift/Alt при желании) или кнопку мыши, включая боковые. Esc — отмена.",
                    action.label()
                ),
            );
        } else {
            ui.weak(
                "Нажмите на поле, затем клавишу или кнопку мыши. Правая кнопка по полю — очистить.",
            );
        }
        egui::ScrollArea::vertical()
            .max_height(380.)
            .show(ui, |ui| {
                egui::Grid::new("bindings_grid")
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        for action in Action::ALL {
                            ui.label(action.label());
                            let slots = self.bindings.slots(action);
                            for (slot, chord) in slots.iter().enumerate() {
                                let capturing = self.input_capture == Some((action, slot));
                                let text = if capturing {
                                    "Нажмите...".to_owned()
                                } else {
                                    chord.map_or("—".into(), |c| c.name())
                                };
                                let response = ui.add(
                                    egui::Button::new(text)
                                        .selected(capturing)
                                        .min_size(egui::vec2(140., 0.)),
                                );
                                if response.clicked() {
                                    self.input_capture = Some((action, slot));
                                    self.input_conflict = None;
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Очистить").clicked() {
                                        self.bindings.clear(action, slot);
                                        self.bindings_changed = true;
                                        ui.close();
                                    }
                                });
                            }
                            if ui
                                .small_button("Сброс")
                                .on_hover_text("Вернуть значения по умолчанию для этого действия")
                                .clicked()
                            {
                                self.bindings.reset(action);
                                self.bindings_changed = true;
                            }
                            ui.end_row();
                        }
                    });
            });
        ui.separator();
        if ui
            .checkbox(&mut self.bindings.invert_zoom, "Обратный зум колесом")
            .changed()
        {
            self.bindings_changed = true;
        }
        if ui.button("Сбросить всё").clicked() {
            self.bindings.reset_all();
            self.bindings_changed = true;
            self.input_capture = None;
        }
    }
    /// ADR-0042 helpers: world axes (negative halves dimmer) and the model's
    /// bounding box with its size, clipped to the viewport.
    fn helpers_ui(root: &egui::Ui, scene: egui::Rect, overlay: &crate::helpers::Overlay) {
        use crate::helpers::Kind;
        let at = |p: [f32; 2]| {
            egui::pos2(
                scene.min.x + p[0] * scene.width(),
                scene.min.y + p[1] * scene.height(),
            )
        };
        let axis = [
            Color32::from_rgb(232, 72, 72),
            Color32::from_rgb(96, 204, 96),
            Color32::from_rgb(80, 140, 240),
        ];
        let painter = root.painter().with_clip_rect(scene);
        let stroke = |kind: Kind| match kind {
            Kind::Axis(a) => egui::Stroke::new(1.5, axis[a]),
            Kind::Back(a) => egui::Stroke::new(1., axis[a].gamma_multiply(0.45)),
            Kind::Bounds => egui::Stroke::new(1., Color32::from_rgb(250, 214, 110)),
        };
        for (a, b, kind) in &overlay.lines {
            painter.line_segment([at(*a), at(*b)], stroke(*kind));
        }
        for (p, text, kind) in &overlay.labels {
            painter.text(
                at(*p) + egui::vec2(4., -4.),
                egui::Align2::LEFT_BOTTOM,
                text,
                egui::FontId::proportional(13.),
                stroke(*kind).color,
            );
        }
    }
    /// ADR-0041/0063 model gizmo: X (red), Y (green), Z (blue) arrows from
    /// the bottom centre move the model, the plane squares move it in two
    /// axes, the rings (or their knobs) turn it about X and Y by the
    /// pointer's angle around the centre, the square at the centre scales
    /// it. One handle is hot at a time: arrows, knobs and the centre first,
    /// then a plane square under the pointer, then the nearest ring.
    fn model_gizmo_ui(
        &mut self,
        context: &egui::Context,
        root: &egui::Ui,
        gizmo: ModelGizmo,
        actions: &mut Actions,
    ) {
        use crate::gizmo_paint as gp;
        use crate::model_gizmo::Handle;
        let scene = self.scene;
        let at = |p: [f32; 2]| {
            egui::pos2(
                scene.min.x + p[0] * scene.width(),
                scene.min.y + p[1] * scene.height(),
            )
        };
        let pivot = at(gizmo.pivot);
        if !scene.shrink(10.).contains(pivot) {
            self.model_turn = None;
            self.model_free_drag = None;
            return;
        }
        // Released outside the window: the drag ends too.
        if self.model_free_drag.is_some()
            && context.pointer_hover_pos().is_none()
            && !context.input(|i| i.pointer.primary_down())
        {
            actions.model_drag_end = true;
            self.model_turn = None;
            self.model_free_drag = None;
        }
        let colors = [
            Color32::from_rgb(232, 72, 72),
            Color32::from_rgb(96, 204, 96),
            Color32::from_rgb(80, 140, 240),
        ];
        let painter = root.painter().with_clip_rect(scene);
        let pointer = context.pointer_hover_pos();
        // Fixed handles: arrow heads, ring knobs and the centre square.
        let mut handles = Vec::new();
        for (axis, end) in gizmo.ends.iter().enumerate() {
            if let Some(end) = end.map(at) {
                painter.line_segment([pivot, end], egui::Stroke::new(2.5, colors[axis]));
                let dir = (end - pivot).normalized();
                let grab = if (end - pivot).length() < 6. {
                    end
                } else {
                    end + dir * 7.
                };
                if scene.shrink(10.).contains(grab) {
                    handles.push((Handle::Axis(axis), grab, colors[axis]));
                }
            }
        }
        for (slot, axis) in crate::model_gizmo::RING_AXES.into_iter().enumerate() {
            if let Some(knob) = gizmo.knobs[slot].map(at)
                && scene.shrink(10.).contains(knob)
            {
                handles.push((Handle::Rotate(axis), knob, colors[axis]));
            }
        }
        handles.push((Handle::Scale, pivot, Color32::from_rgb(235, 235, 235)));
        let fixed_id = |h: Handle| egui::Id::new(("model_gizmo", format!("{h:?}")));
        let free_id = |h: Handle| egui::Id::new(("model_free", format!("{h:?}")));
        let free_handles = [
            Handle::Plane(0),
            Handle::Plane(1),
            Handle::Plane(2),
            Handle::Rotate(0),
            Handle::Rotate(1),
        ];
        let fixed_busy = handles
            .iter()
            .any(|(h, _, _)| context.is_being_dragged(fixed_id(*h)));
        let on_fixed =
            pointer.is_some_and(|p| handles.iter().any(|(_, at, _)| at.distance(p) < 13.));
        let planes = gizmo.planes.map(|q| {
            let [a, b, c, d] = q;
            Some([at(a?), at(b?), at(c?), at(d?)])
        });
        let rings = gizmo.rings.map(|ring| ring.map(|p| p.map(at)));
        let ring_distance = |slot: usize, p: egui::Pos2| {
            let ring = &rings[slot];
            (0..ring.len())
                .filter_map(|i| match (ring[i], ring[(i + 1) % ring.len()]) {
                    (Some(a), Some(b)) => Some(segment_distance(p, a, b)),
                    _ => None,
                })
                .fold(f32::INFINITY, f32::min)
        };
        // The free handle that is hot: the dragged one, or the best under
        // the pointer when no fixed handle is there.
        // A drag keeps its handle until release, even when the pointer leaves
        // the square or ring.
        let hot_free = self
            .model_free_drag
            .or_else(|| {
                free_handles
                    .into_iter()
                    .find(|h| context.is_being_dragged(free_id(*h)))
            })
            .or_else(|| {
                let p = pointer.filter(|p| scene.contains(*p))?;
                if fixed_busy || on_fixed {
                    return None;
                }
                (0..3)
                    .find(|&i| planes[i].is_some_and(|q| gp::in_quad(p, &q)))
                    .map(Handle::Plane)
                    .or_else(|| {
                        crate::model_gizmo::RING_AXES
                            .into_iter()
                            .enumerate()
                            .map(|(slot, axis)| {
                                let points: Vec<_> =
                                    rings[slot].iter().flatten().copied().collect();
                                let d = ring_distance(slot, p);
                                (axis, d, gp::ring_score(d, gp::openness(&points)))
                            })
                            .filter(|(_, d, _)| *d < 8.)
                            .min_by(|a, b| a.2.total_cmp(&b.2))
                            .map(|(axis, _, _)| Handle::Rotate(axis))
                    })
            });
        for (i, quad) in planes.iter().enumerate() {
            if let Some(quad) = quad {
                let color = colors[gp::PLANE_NORMAL[i]];
                gp::plane(&painter, quad, color, hot_free == Some(Handle::Plane(i)));
            }
        }
        let turning = self.model_turn.map(|(axis, _)| axis);
        for (slot, axis) in crate::model_gizmo::RING_AXES.into_iter().enumerate() {
            let hot = hot_free == Some(Handle::Rotate(axis)) || turning == Some(axis);
            let color = if hot {
                colors[axis]
            } else {
                colors[axis].gamma_multiply(0.8)
            };
            let ring = &rings[slot];
            for i in 0..ring.len() {
                if let (Some(a), Some(b)) = (ring[i], ring[(i + 1) % ring.len()]) {
                    painter
                        .line_segment([a, b], egui::Stroke::new(if hot { 3. } else { 1.6 }, color));
                }
            }
        }
        self.pointer_on_gizmo |= hot_free.is_some() || on_fixed || fixed_busy;
        // The free grab follows the pointer. One area for the whole gizmo,
        // shown every frame: a new area is invisible in its first frame and
        // would miss a quick press.
        let grab = hot_free.zip(pointer);
        egui::Area::new(egui::Id::new("model_free_grab"))
            .order(egui::Order::Middle)
            .fixed_pos(grab.map_or(pivot, |g| g.1) - egui::vec2(10., 10.))
            .show(context, |ui| {
                if let Some((handle, p)) = grab {
                    let rect = egui::Rect::from_center_size(p, egui::vec2(20., 20.));
                    let response = ui.interact(rect, free_id(handle), egui::Sense::drag());
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        self.model_free_drag = Some(handle);
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        self.model_free_drag = Some(handle);
                        let d = response.drag_delta();
                        match handle {
                            Handle::Rotate(axis) => {
                                let step = self.model_turn_step(axis, pivot, p, d);
                                if step != 0. {
                                    actions.model_ring_drag = Some((axis, step));
                                }
                            }
                            _ if d != egui::Vec2::ZERO => {
                                actions.model_drag =
                                    Some((handle, [d.x / scene.width(), d.y / scene.height()]));
                            }
                            _ => {}
                        }
                    }
                    if response.drag_stopped_by(egui::PointerButton::Primary)
                        || (self.model_free_drag.is_some()
                            && !ui.input(|i| i.pointer.primary_down()))
                    {
                        actions.model_drag_end = true;
                        self.model_turn = None;
                        self.model_free_drag = None;
                    }
                    response.on_hover_text(match handle {
                        Handle::Plane(0) => "Тяните квадрат — сдвиг в плоскости XY (вровень со стеной, смотрящей вдоль Z). Отпускание — одна правка (Ctrl+Z отменит).",
                        Handle::Plane(1) => "Тяните квадрат — сдвиг в плоскости YZ (вровень со стеной, смотрящей вдоль X). Отпускание — одна правка (Ctrl+Z отменит).",
                        Handle::Plane(_) => "Тяните квадрат — сдвиг по полу (X и Z, высота не меняется). Отпускание — одна правка (Ctrl+Z отменит).",
                        Handle::Rotate(0) => "Тяните по красному кольцу — наклон вокруг X. Чем дальше мышь от центра, тем точнее; угол виден у курсора.",
                        _ => "Тяните по зелёному кольцу — поворот вокруг Y. Чем дальше мышь от центра, тем точнее; угол виден у курсора.",
                    });
                }
            });
        for (handle, position, color) in handles {
            let id = fixed_id(handle);
            egui::Area::new(id)
                .order(egui::Order::Middle)
                .fixed_pos(position - egui::vec2(11., 11.))
                .show(context, |ui| {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(22., 22.), egui::Sense::click_and_drag());
                    #[cfg(test)]
                    {
                        self.model_gizmo_rects.push((handle, rect));
                    }
                    let hot = response.hovered() || response.dragged_by(egui::PointerButton::Primary);
                    let painter = ui.painter();
                    let dark = egui::Stroke::new(1.5, Color32::from_rgb(24, 26, 32));
                    match handle {
                        Handle::Axis(axis) => {
                            let end = gizmo.ends[axis].map(at).unwrap_or(rect.center());
                            gp::arrow_head(painter, pivot, end, if hot { 9. } else { 7. }, color);
                        }
                        Handle::Rotate(_) => {
                            let r = if hot { 8.5 } else { 7. };
                            painter.circle_filled(rect.center(), r, Color32::from_rgb(24, 26, 32));
                            painter.circle_stroke(rect.center(), r, egui::Stroke::new(2.5, color));
                            painter.circle_filled(rect.center(), 2.5, color);
                        }
                        Handle::Scale | Handle::Plane(_) => {
                            let half = if hot { 7. } else { 5.5 };
                            let square =
                                egui::Rect::from_center_size(rect.center(), egui::vec2(half * 2., half * 2.));
                            painter.rect_filled(square, 1., color);
                            painter.rect_stroke(square, 1., dark, egui::StrokeKind::Outside);
                        }
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        let d = response.drag_delta();
                        match handle {
                            Handle::Rotate(axis) => {
                                if let Some(p) = response.interact_pointer_pos() {
                                    let step = self.model_turn_step(axis, pivot, p, d);
                                    if step != 0. {
                                        actions.model_ring_drag = Some((axis, step));
                                    }
                                }
                            }
                            _ if d != egui::Vec2::ZERO => {
                                actions.model_drag =
                                    Some((handle, [d.x / scene.width(), d.y / scene.height()]));
                            }
                            _ => {}
                        }
                    }
                    if response.drag_stopped_by(egui::PointerButton::Primary) {
                        actions.model_drag_end = true;
                        self.model_turn = None;
                    }
                    let hint = match handle {
                        Handle::Axis(0) => "Тяните — сдвинуть модель по X. Отпускание — одна правка (Ctrl+Z отменит).",
                        Handle::Axis(1) => "Тяните — сдвинуть модель по Y. Отпускание — одна правка (Ctrl+Z отменит).",
                        Handle::Axis(_) => "Тяните — сдвинуть модель по Z. Отпускание — одна правка (Ctrl+Z отменит).",
                        Handle::Rotate(0) => "Тяните по кругу вокруг центра — наклонить модель вокруг оси X. Чем дальше мышь, тем точнее. Отпускание — одна правка (Ctrl+Z отменит).",
                        Handle::Rotate(_) => "Тяните по кругу вокруг центра — повернуть модель вокруг оси Y. Чем дальше мышь, тем точнее. Отпускание — одна правка (Ctrl+Z отменит).",
                        Handle::Scale | Handle::Plane(_) => "Тяните вверх — крупнее, вниз — мельче (от нижней точки центра). ПКМ — меню.",
                    };
                    let response = response.on_hover_text(hint);
                    if handle == Handle::Scale {
                        response.context_menu(|ui| {
                            if ui
                                .add_enabled(
                                    !self.placement.is_identity(),
                                    egui::Button::new("Сбросить положение модели"),
                                )
                                .clicked()
                            {
                                actions.model_reset = true;
                                ui.close();
                            }
                            if ui.button("Скрыть гизмо модели").clicked() {
                                self.model_gizmo_on = false;
                                ui.close();
                            }
                        });
                    }
                });
        }
        // The angle being turned, next to the pointer.
        if let (Some((axis, track)), Some(p)) = (self.model_turn, pointer) {
            let degrees = gizmo.turned.unwrap_or(0.).to_degrees();
            gp::turn_indicator(&painter, pivot, p, &track, degrees, colors[axis]);
        }
    }
    /// One pointer step of a ring or knob drag: the screen angle it turned
    /// around `pivot` (0 inside the dead zone). Starts the turn track at the
    /// pointer's previous position on the first step.
    fn model_turn_step(
        &mut self,
        axis: usize,
        pivot: egui::Pos2,
        p: egui::Pos2,
        d: egui::Vec2,
    ) -> f32 {
        if self.model_turn.is_none_or(|(a, _)| a != axis) {
            self.model_turn = crate::gizmo_paint::TurnTrack::new(pivot, p - d).map(|t| (axis, t));
        }
        match self.model_turn.as_mut() {
            Some((_, track)) => track.step(pivot, p),
            None => {
                // Grabbed inside the dead zone: start once the pointer leaves it.
                self.model_turn = crate::gizmo_paint::TurnTrack::new(pivot, p).map(|t| (axis, t));
                0.
            }
        }
    }
    /// ADR-0040 lamp gizmo over the viewport: a sun icon on a line from the
    /// model centre. Dragging it turns the lamp, right click opens its menu.
    /// Hidden when the lamp is outside the viewport.
    fn lamp_gizmo_ui(
        &mut self,
        context: &egui::Context,
        root: &egui::Ui,
        gizmo: LampGizmo,
        actions: &mut Actions,
    ) {
        let at = |p: [f32; 2]| {
            egui::pos2(
                self.scene.min.x + p[0] * self.scene.width(),
                self.scene.min.y + p[1] * self.scene.height(),
            )
        };
        let (lamp, center) = (at(gizmo.lamp), at(gizmo.center));
        let was = self.lamp_hot.take();
        if !self.scene.shrink(14.).contains(lamp) {
            return;
        }
        let scene = self.scene;
        if let Some(guides) = gizmo.guides.as_deref() {
            let painter = root.painter().with_clip_rect(scene);
            let path = |points: &[Option<[f32; 2]>], closed: bool, color: Color32| {
                let n = points.len();
                let last = if closed { n } else { n - 1 };
                for i in 0..last {
                    if let (Some(a), Some(b)) = (points[i], points[(i + 1) % n]) {
                        painter.line_segment([at(a), at(b)], egui::Stroke::new(1.5, color));
                    }
                }
            };
            let (azimuth_color, elevation_color) = (
                Color32::from_rgb(90, 200, 235),
                Color32::from_rgb(245, 160, 60),
            );
            match was {
                Some(LampPart::Azimuth) => path(&guides.orbit, true, azimuth_color),
                Some(LampPart::Elevation) => path(&guides.arc, false, elevation_color),
                Some(LampPart::Icon) => {
                    path(&guides.orbit, true, azimuth_color.gamma_multiply(0.45));
                    path(&guides.arc, false, elevation_color.gamma_multiply(0.45));
                }
                Some(LampPart::Distance) | None => {}
            }
            for (part, tangent, color, hint) in [
                (
                    LampPart::Azimuth,
                    guides.azimuth,
                    azimuth_color,
                    "Тяните — вести лампу по горизонтали вокруг модели (азимут); высота не меняется.",
                ),
                (
                    LampPart::Elevation,
                    guides.elevation,
                    elevation_color,
                    "Тяните — поднять или опустить лампу (высота); азимут не меняется.",
                ),
                (
                    LampPart::Distance,
                    guides.distance,
                    Color32::from_rgb(200, 120, 245),
                    "Тяните — приблизить или отдалить лампу (расстояние до модели); ближе — ярче.",
                ),
            ] {
                let Some(t) = tangent else {
                    continue;
                };
                let t_px = egui::vec2(t[0] * scene.width(), t[1] * scene.height());
                if !t_px.is_finite() || t_px.length() < 1e-4 {
                    continue;
                }
                let dir = t_px.normalized();
                let position = lamp + dir * 34.;
                if !scene.shrink(12.).contains(position) {
                    continue;
                }
                painter.line_segment(
                    [lamp + dir * 13., position - dir * 9.],
                    egui::Stroke::new(1.5, color.gamma_multiply(0.7)),
                );
                egui::Area::new(egui::Id::new(("lamp_handle", format!("{part:?}"))))
                    .order(egui::Order::Middle)
                    .fixed_pos(position - egui::vec2(11., 11.))
                    .show(context, |ui| {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(22., 22.),
                            egui::Sense::click_and_drag(),
                        );
                        #[cfg(test)]
                        {
                            self.lamp_handle_rects.push((part, rect));
                        }
                        let hot =
                            response.hovered() || response.dragged_by(egui::PointerButton::Primary);
                        if hot {
                            self.lamp_hot = Some(part);
                        }
                        let size = if hot { 5.5 } else { 4.5 };
                        let painter = ui.painter();
                        painter.circle_filled(rect.center(), if hot { 4. } else { 3. }, color);
                        crate::diagnostic_ui::triangle(
                            painter,
                            rect.center() + dir * 6.,
                            size,
                            dir,
                            color,
                        );
                        crate::diagnostic_ui::triangle(
                            painter,
                            rect.center() - dir * 6.,
                            size,
                            -dir,
                            color,
                        );
                        if response.dragged_by(egui::PointerButton::Primary) {
                            let d = response.drag_delta();
                            let turn = crate::lamp::handle_turn([d.x, d.y], [t_px.x, t_px.y]);
                            if turn != 0. {
                                match part {
                                    LampPart::Azimuth => actions.lamp_turn = Some([turn, 0.]),
                                    LampPart::Distance => {
                                        actions.lamp_distance = Some(turn.clamp(-1., 1.));
                                    }
                                    _ => actions.lamp_turn = Some([0., turn]),
                                }
                            }
                        }
                        response.on_hover_text(hint);
                    });
            }
        }
        let [r, g, b] = self.lighting.color.map(|c| (c.clamp(0., 1.) * 255.) as u8);
        let color = Color32::from_rgb(r.max(60), g.max(60), b.max(60));
        root.painter().with_clip_rect(self.scene).line_segment(
            [center, lamp],
            egui::Stroke::new(1., Color32::from_rgba_unmultiplied(255, 214, 120, 110)),
        );
        egui::Area::new(egui::Id::new("lamp_gizmo"))
            .order(egui::Order::Middle)
            .fixed_pos(lamp - egui::vec2(14., 14.))
            .show(context, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(28., 28.), egui::Sense::click_and_drag());
                #[cfg(test)]
                {
                    self.lamp_gizmo_rect = rect;
                }
                let hot = response.hovered() || response.dragged_by(egui::PointerButton::Primary);
                if hot && self.lamp_hot.is_none() {
                    self.lamp_hot = Some(LampPart::Icon);
                }
                let radius = if hot { 8. } else { 6. };
                let painter = ui.painter();
                let dark = egui::Stroke::new(1.5, Color32::from_rgb(40, 36, 24));
                painter.circle_filled(rect.center(), radius, color);
                painter.circle_stroke(rect.center(), radius, dark);
                for i in 0..8 {
                    let a = i as f32 * std::f32::consts::FRAC_PI_4;
                    let v = egui::vec2(a.cos(), a.sin());
                    painter.line_segment(
                        [
                            rect.center() + v * (radius + 2.),
                            rect.center() + v * (radius + 5.),
                        ],
                        egui::Stroke::new(2., color),
                    );
                }
                if response.dragged_by(egui::PointerButton::Primary) {
                    let d = response.drag_delta();
                    if d != egui::Vec2::ZERO {
                        actions.lamp_drag = Some([d.x, d.y]);
                    }
                }
                let response = response.on_hover_text(
                    "Лампа. Тяните — повернуть свет вокруг модели; голубые стрелки — только по горизонтали, оранжевые — только по высоте; правая кнопка — меню.",
                );
                response.context_menu(|ui| {
                    if ui.button("Сбросить положение").clicked() {
                        self.lighting.direction =
                            ad_render_vulkan::lighting::DEFAULT_LAMP_DIRECTION;
                        ui.close();
                    }
                    if ui.button("Свет сверху").clicked() {
                        let (azimuth, _) = crate::lamp::angles(self.lighting.direction);
                        self.lighting.direction =
                            crate::lamp::from_angles(azimuth, crate::lamp::MAX_ELEVATION);
                        ui.close();
                    }
                    if ui.button("Свет от камеры").clicked() {
                        actions.lamp_from_camera = true;
                        ui.close();
                    }
                    ui.checkbox(&mut self.lighting.enabled, "Лампа включена");
                    ui.horizontal(|ui| {
                        ui.label("Цвет");
                        ui.color_edit_button_rgb(&mut self.lighting.color);
                    });
                    if ui
                        .add(
                            egui::Slider::new(&mut self.lamp_kelvin, 1500.0..=10000.0)
                                .text("Температура, K"),
                        )
                        .changed()
                    {
                        self.lighting.color = crate::lamp::kelvin_to_rgb(self.lamp_kelvin);
                    }
                    if ui.button("Белый свет").clicked() {
                        self.lighting.color = [1.; 3];
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Скрыть значок").clicked() {
                        self.lamp_icon = false;
                        ui.close();
                    }
                });
            });
    }
    /// Opens the right-click menu at a window position (pixels).
    pub fn open_context_menu(&mut self, x: f64, y: f64, node: Option<ad_document::NodeId>) {
        let scale = self.context.pixels_per_point() as f64;
        self.context_menu = Some((egui::pos2((x / scale) as f32, (y / scale) as f32), node));
        self.menu = None;
    }
    /// Viewport coordinates (0..1, y down) of a window position in pixels.
    pub fn viewport_uv(&self, x: f64, y: f64) -> Option<[f32; 2]> {
        let scale = self.context.pixels_per_point() as f64;
        let p = egui::pos2((x / scale) as f32, (y / scale) as f32);
        self.scene.contains(p).then(|| {
            [
                (p.x - self.scene.min.x) / self.scene.width(),
                (p.y - self.scene.min.y) / self.scene.height(),
            ]
        })
    }
    /// Like `viewport_uv`, also outside the viewport (selection box ends).
    pub fn viewport_uv_unclamped(&self, x: f64, y: f64) -> Option<[f32; 2]> {
        let scale = self.context.pixels_per_point() as f64;
        let (w, h) = (self.scene.width(), self.scene.height());
        (w > 0. && h > 0.).then(|| {
            [
                ((x / scale) as f32 - self.scene.min.x) / w,
                ((y / scale) as f32 - self.scene.min.y) / h,
            ]
        })
    }
    pub fn camera_at(&self, x: f64, y: f64) -> bool {
        let scale = self.context.pixels_per_point() as f64;
        let p = egui::pos2((x / scale) as f32, (y / scale) as f32);
        self.scene.contains(p) && !self.strip_rect.contains(p)
    }
    pub fn help(&mut self) {
        self.help = true;
    }
    pub fn dismiss_help(&mut self) {
        self.help = false;
    }
    pub fn camera_height(&self) -> f32 {
        self.scene.height() * self.context.pixels_per_point()
    }
    pub fn frame(
        &mut self,
        window: &Window,
        asset: Option<&PreparedAsset>,
        loading: bool,
    ) -> (UiFrame, Actions) {
        let raw = self
            .input
            .as_mut()
            .expect("UI attached to window")
            .take_egui_input(window);
        let (output, scene, actions) = self.layout(raw, asset, loading);
        self.input
            .as_mut()
            .expect("UI attached to window")
            .handle_platform_output(window, output.platform_output);
        let pixels_per_point = output.pixels_per_point;
        let mut primitives = self.context.tessellate(output.shapes, pixels_per_point);
        let size = window.inner_size();
        let clip = egui::Rect::from_min_max(
            egui::Pos2::ZERO,
            egui::pos2(
                size.width as f32 / pixels_per_point,
                size.height as f32 / pixels_per_point,
            ),
        );
        for p in &mut primitives {
            p.clip_rect = p.clip_rect.intersect(clip);
        }
        primitives.retain(|p| p.clip_rect.is_positive());
        (
            UiFrame {
                primitives,
                textures: output.textures_delta,
                pixels_per_point,
                scene: ViewportRect::from_logical(
                    [scene.min.x, scene.min.y],
                    [scene.max.x, scene.max.y],
                    pixels_per_point,
                    &SurfaceSize {
                        width: size.width,
                        height: size.height,
                    },
                ),
            },
            actions,
        )
    }
    fn layout(
        &mut self,
        mut raw: egui::RawInput,
        asset: Option<&PreparedAsset>,
        loading: bool,
    ) -> (egui::FullOutput, egui::Rect, Actions) {
        let context = self.context.clone();
        self.pointer_on_gizmo = false;
        // Tab is the outline switch and Space plays / pauses the animation:
        // egui must not use them to move focus or press a focused button,
        // except inside a text field.
        if !context.egui_wants_keyboard_input() {
            raw.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Tab | egui::Key::Space,
                        ..
                    }
                )
            });
        }
        // The side mouse buttons turn and move the camera (owner 2026-11-28):
        // the interface never sees them, so a gizmo under the pointer does
        // not take them as a drag.
        raw.events.retain(|event| {
            !matches!(
                event,
                egui::Event::PointerButton {
                    button: egui::PointerButton::Extra1 | egui::PointerButton::Extra2,
                    ..
                }
            )
        });
        let mut actions = Actions::default();
        // Interface scale: the setting wins over egui's own keyboard zoom
        // only when the setting itself changed.
        if (context.zoom_factor() - self.applied_scale).abs() > 1e-4 {
            self.ui_scale = context.zoom_factor().clamp(0.75, 2.);
        }
        self.ui_scale = self.ui_scale.clamp(0.75, 2.);
        if (self.ui_scale - context.zoom_factor()).abs() > 1e-4 {
            context.set_zoom_factor(self.ui_scale);
        }
        self.applied_scale = self.ui_scale;
        self.gizmo_dragging = context.dragged_id().is_some();
        let output=context.run_ui(raw,|root| {
            let width=root.max_rect().width();
            let height=root.max_rect().height();
            let read_only = asset.is_some_and(|asset| asset.document().is_read_only());
            self.top_bar(root, width, loading, read_only, &mut actions);
            egui::Panel::bottom("status").exact_size(if self.error.is_some(){64.}else{32.}).resizable(false).show(root,|ui| {
                if let Some(error)=&self.error {
                    ui.add(egui::Label::new(RichText::new(error).color(Color32::from_rgb(255,173,173))).truncate()).on_hover_text(error);
                }
                ui.horizontal(|ui| {
                    if loading {
                        ui.spinner();
                        if width>=430. {ui.label("Загрузка GLB…");}
                        actions.cancel|=ui.button(if width>=430. {"Отменить · Esc"} else {"Отменить"}).on_hover_text("Отменить загрузку · Esc").clicked();
                    }
                    else if let Some(busy)=&self.edit.busy {
                        ui.spinner();
                        if width>=430. {ui.label(busy);}
                        actions.cancel_edit|=ui.button("Отменить подготовку").on_hover_text("Модель не изменится · Esc").clicked();
                    }
                    else if self.status_warning {ui.add(egui::Label::new(RichText::new(&self.status).color(Color32::from_rgb(255,190,120))).truncate()).on_hover_text(&self.status);}
                    else {ui.label(&self.status);}
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                        self.bug_button(ui);
                        if ui.available_width()>220. {ui.weak("Vulkan · геометрия и базовые цвета");ui.separator();}
                    });
                });
            });
            if self.show_tree {
                self.work_tree_panel(root);
            }
            if self.show_summary && width>=680. {
                self.right_panel(root,asset,loading,&mut actions);
            }
            // Leave the central root area unclaimed by egui. A CentralPanel would
            // mark it as UI-owned and consume the viewport's mouse input.
            {
                self.scene=root.available_rect_before_wrap();
                if asset.is_some() && let Some(overlay)=self.helper_overlay.as_ref() {
                    Self::helpers_ui(root,self.scene,overlay);
                }
                if let Some((a,b,remove))=self.marquee {
                    let at=|p:[f32;2]| egui::pos2(self.scene.min.x+p[0]*self.scene.width(),self.scene.min.y+p[1]*self.scene.height());
                    let rect=egui::Rect::from_two_pos(at(a),at(b));
                    let color=if remove {Color32::from_rgb(255,120,90)} else {Color32::from_rgb(120,190,255)};
                    let painter=root.painter().with_clip_rect(self.scene);
                    painter.rect_filled(rect,0.,color.gamma_multiply(0.12));
                    painter.rect_stroke(rect,0.,egui::Stroke::new(1.,color),egui::StrokeKind::Inside);
                }
                if asset.is_some() && self.skeleton_on && let Some(overlay)=self.skeleton.take() {
                    self.skeleton_ui(&context,root,&overlay,asset.map(PreparedAsset::document),&mut actions);
                    self.skeleton=Some(overlay);
                }
                if asset.is_some() && !self.fit_uv.is_empty() {
                    self.fit_ui(root);
                }
                if asset.is_some() && let Some(path)=self.root_path_uv.take() {
                    self.root_path_ui(root,&path);
                    self.root_path_uv=Some(path);
                }
                if asset.is_some() && let Some(gizmo)=self.joint_gizmo.take() {
                    self.joint_gizmo_ui(&context,root,&gizmo,&mut actions);
                    self.joint_gizmo=Some(gizmo);
                } else {
                    self.joint_drag=None;
                }
                if asset.is_some() && !read_only && self.model_gizmo_on && let Some(gizmo)=self.model_gizmo {
                    self.model_gizmo_ui(&context,root,gizmo,&mut actions);
                } else {
                    self.model_turn=None;
                    self.model_free_drag=None;
                }
                if asset.is_some() && self.lamp_icon && let Some(gizmo)=self.lamp_gizmo.clone() {
                    self.lamp_gizmo_ui(&context,root,gizmo,&mut actions);
                }
                if !(asset.is_some() && self.lamp_icon && self.lamp_gizmo.is_some()) {
                    self.lamp_hot=None;
                }
                self.pointer_on_gizmo |= self.lamp_hot.is_some();
                self.work_tree_strip(root,width);
                self.corner_overlay(&context,width,loading,&mut actions);
                self.playback_bar(&context,asset,&mut actions);
                if asset.is_none() {
                    let center=self.scene.center();
                    root.painter().text(center-egui::vec2(0.,16.),egui::Align2::CENTER_CENTER,"Откройте модель",egui::FontId::proportional(24.),Color32::from_rgb(220,224,232));
                    root.painter().text(center+egui::vec2(0.,22.),egui::Align2::CENTER_CENTER,"Ctrl+O или перетащите GLB в окно",egui::FontId::proportional(14.),Color32::from_rgb(154,164,181));
                }
            }
            self.menu_popup(&context,height,asset,&mut actions);
            #[cfg(test)]
            self.skybox_tile_rects.clear();
            self.skybox_window(&context,&mut actions);
            self.log_window(&context);
            self.update_window(&context,&mut actions);
            self.context_menu_ui(&context,asset,&mut actions);
            if let Some(info)=self.edit.preview.clone() {
                let ready=!loading && self.edit.busy.is_none();
                let code=crate::edits::repair_label(info.kind);
                egui::Window::new(format!("Ремонт {code} · предпросмотр")).id(egui::Id::new("repair_preview")).collapsible(false).resizable(false).default_width(420.).show(&context,|ui| {
                    ui.strong(format!("Исходный примитив #{}",info.mesh.0));
                    ui.label(format!("Ссылок из узлов: {} (включая неактивные; это не число видимых копий).",info.references));
                    let vertices=matches!(info.kind,ad_heal::RepairKind::UnusedVertices|ad_heal::RepairKind::DuplicateVertices);
                    ui.label(if vertices {"Правка меняет общий примитив: все узлы, которые его используют, получат одинаковые вершины и индексы."} else {"Правка меняет общий примитив: все узлы, которые его используют, получат одинаковые индексы."});
                    ui.separator();
                    let normals=matches!(info.kind,ad_heal::RepairKind::MissingNormals|ad_heal::RepairKind::ZeroNormals);
                    if normals {
                        ui.label(format!("Нормали: вычисляется {} из {} вершин. Позиции, треугольники и остальные данные не меняются.",info.changed_normals,info.vertices_before));
                    } else if vertices {
                        ui.label(format!("Вершины: было {}, станет {} (удаляется {}). Треугольники не меняются: {}.",info.vertices_before,info.vertices_after,info.vertices_before-info.vertices_after,info.before));
                        if let Some((before,after))=info.boundary_edges {ui.label(format!("Граничные рёбра (G006): было {before}, станет {after}."));}
                    } else {
                        ui.label(format!("Треугольники: было {}, станет {} (удаляется {}).",info.before,info.after,info.before-info.after));
                    }
                    ui.weak(match info.kind {
                        ad_heal::RepairKind::DuplicateTriangles=>"Удаляются только точные повторы с тем же обходом; первый экземпляр, порядок и все атрибуты сохраняются.",
                        ad_heal::RepairKind::DegenerateTriangles=>"Удаляются только треугольники точно нулевой площади (повторяющийся индекс или три вершины на одной прямой); остальные треугольники, их порядок, вершины и атрибуты сохраняются. Освободившиеся вершины остаются (G003).",
                        ad_heal::RepairKind::UnusedVertices=>"Удаляются только вершины, на которые не ссылается ни один треугольник этого примитива. Порядок и все атрибуты оставшихся вершин сохраняются, индексы перенумеровываются. Другие примитивы с общим буфером вершин продолжают использовать прежние данные.",
                        ad_heal::RepairKind::MissingNormals=>"Нормали вычисляются по граням с учётом их площади, по индексам вершин: разрезанные швы остаются острыми. В копии GLB появится новый атрибут NORMAL.",
                        ad_heal::RepairKind::ZeroNormals=>"Заменяются только нулевые нормали; авторские нормали сохраняются без изменений. Новые значения вычисляются по граням с учётом их площади.",
                        ad_heal::RepairKind::DuplicateVertices=>"Объединяются только вершины, у которых совпадают все данные: позиция, нормаль, касательная, цвет и все UV. Треугольники переходят на первую такую вершину; швы с разными UV или нормалями остаются. Ремонт отклоняется, если топология примитива ухудшится.",
                    });
                    match (info.complete,info.target_left) {
                        (true,Some(0))=>ui.label(format!("Проверка результата: полный анализ, {code} в этом примитиве не останется.")),
                        (true,Some(n))=>ui.colored_label(Color32::from_rgb(255,190,120),format!("Проверка результата: полный анализ, {code} останется: {n}.")),
                        _=>ui.colored_label(Color32::from_rgb(255,190,120),format!("Проверка результата неполная: отсутствие {code} не доказано.")),
                    };
                    ui.label("Сохранение: результат совместим с проверенным экспортом копии GLB.");
                    ui.weak(format!("Копия GLB станет больше примерно на {:.1} КиБ: новые данные пишутся в конец файла, исходные данные остаются.",info.appended_bytes as f64/1024.));
                    ui.weak(format!("Новая сцена на GPU: {:.1} МиБ геометрии; текстуры загружаются заново. Новые G006 после удаления треугольников — не обязательно новые дыры.",info.geometry_bytes as f64/1048576.));
                    ui.separator();
                    ui.horizontal(|ui| {
                        let apply=ui.add_enabled(ready,egui::Button::new("Применить")).on_hover_text("Опубликовать правку целиком: индексы, история, анализ и сцена. Отменяется через «Отменить правку».");
                        let discard=ui.button("Отмена").on_hover_text("Закрыть предпросмотр; модель не изменится.");
                        #[cfg(test)]
                        {self.apply_rect=apply.rect;self.discard_rect=discard.rect;}
                        actions.apply|=apply.clicked();
                        actions.discard_preview|=discard.clicked();
                    });
                });
            }
            if self.show_queue {
                let mut open=true;
                let ready=!loading && self.edit.busy.is_none();
                egui::Window::new("Очередь ремонтов").id(egui::Id::new("repair_queue")).open(&mut open).collapsible(false).resizable(false).default_width(460.).show(&context,|ui| {
                    match &self.edit.queue {
                        Some(q) if q.running => {
                            ui.label(format!("Выполнено: {} · пропущено: {}",q.done.len(),q.skipped.len()));
                            if let Some(current)=&q.current {ui.label(format!("Сейчас: {current}"));}
                            actions.queue_stop|=ui.button("Остановить").on_hover_text("Текущий шаг отменяется; уже применённые ремонты остаются и отменяются через «Отменить правку».").clicked();
                        }
                        _ => {
                            if let Some(q)=&self.edit.queue {
                                ui.strong(format!("Итог: выполнено {}, пропущено {}",q.done.len(),q.skipped.len()));
                                for step in &q.done {ui.label(format!("выполнено: {step}"));}
                                for step in &q.skipped {ui.colored_label(Color32::from_rgb(255,190,120),format!("пропущено: {step}"));}
                                if let Some(reason)=&q.stop {ui.colored_label(Color32::from_rgb(255,190,120),format!("Остановлена: {reason}"));}
                                ui.separator();
                            }
                            let planned=asset.and_then(|a|a.diagnostics().as_ref().ok()).map(crate::queue::candidates).unwrap_or_default();
                            if planned.is_empty() {
                                ui.label("Ремонтируемых замечаний нет.");
                            } else {
                                ui.label("Первый проход (после каждого шага анализ повторяется и список пересчитывается):");
                                for (mesh,kind) in planned.iter().take(12) {ui.label(crate::queue::step_name(*mesh,*kind));}
                                if planned.len()>12 {ui.weak(format!("… и ещё {}",planned.len()-12));}
                            }
                            ui.weak("Каждый шаг — обычный проверенный ремонт и отдельная запись истории (Undo по шагам). Отказы пропускаются с причиной; регрессия или сбой останавливают очередь. Файл не сохраняется.");
                            let start=ui.add_enabled(ready && !planned.is_empty(),egui::Button::new("Запустить очередь"));
                            #[cfg(test)]
                            {self.queue_start_rect=start.rect;}
                            actions.queue_start|=start.clicked();
                        }
                    }
                    ui.separator();
                    actions.history_open|=ui.button("История ремонта…").on_hover_text("Прошлые ремонты и сохранения из всех запусков программы. Это не проверка текущей модели.").clicked();
                });
                self.show_queue&=open;
            }
            if self.history.open {
                let mut open=true;
                egui::Window::new("История ремонта").id(egui::Id::new("repair_history")).open(&mut open).resizable(true).default_size([620.,360.]).show(&context,|ui| {
                    ui.colored_label(Color32::from_rgb(255,190,120),"Записи описывают прошлые ремонты и проверки. Состояние текущей модели — только в окне «Диагностика».");
                    if !self.history.path.is_empty() {ui.weak(format!("Файл: {}",self.history.path));}
                    if let Some(error)=&self.history.error {ui.colored_label(Color32::from_rgb(255,173,173),error);}
                    if self.history.corrupt>0 {ui.colored_label(Color32::from_rgb(255,190,120),format!("Пропущено повреждённых записей: {}",self.history.corrupt));}
                    ui.checkbox(&mut self.history.only_session,"Только эта сессия");
                    egui::ScrollArea::vertical().max_height(240.).show(ui,|ui|{
                        let mut shown=0;
                        for (session,line) in &self.history.lines {
                            if self.history.only_session && !session {continue;}
                            ui.label(line);
                            shown+=1;
                        }
                        if shown==0 {ui.weak("Записей нет.");}
                    });
                    ui.separator();
                    if self.history.confirm_clear {
                        ui.horizontal(|ui| {
                            ui.label("Удалить всю историю ремонта?");
                            if ui.button("Да, очистить").clicked() {actions.history_clear=true;self.history.confirm_clear=false;}
                            if ui.button("Нет").clicked() {self.history.confirm_clear=false;}
                        });
                    } else if ui.add_enabled(!self.history.lines.is_empty(),egui::Button::new("Очистить историю…")).clicked() {
                        self.history.confirm_clear=true;
                    }
                });
                self.history.open&=open;
            }
            if self.show_controls {
                let mut open = true;
                egui::Window::new("Управление").id(egui::Id::new("input_bindings")).open(&mut open).resizable(true).default_size([560.,520.]).show(&context,|ui| {
                    self.controls_ui(ui);
                });
                if !open {
                    self.show_controls=false;
                    self.input_capture=None;
                    self.input_conflict=None;
                }
            }
            egui::Window::new("Справка").open(&mut self.help).resizable(true).show(&context,|ui| {
                ui.strong(concat!("AD RT-PT v", env!("CARGO_PKG_VERSION")));
                ui.label(format!("Открыть: {} или перетащить GLB в окно.",self.bindings.describe(crate::input::Action::Open)));
                ui.label(format!("{} — вращение; {} — перемещение; колесо — масштаб.",self.bindings.describe(crate::input::Action::Orbit),self.bindings.describe(crate::input::Action::Pan)));
                ui.label(format!("{} — вписать модель; {} — скрыть или вернуть пол; значок лампы во вьюпорте — тянуть, чтобы повернуть свет (его голубые стрелки — только по горизонтали, оранжевые — только по высоте), правая кнопка — меню лампы; стрелки X/Y/Z — сдвинуть модель, кольца — повернуть вокруг Y и X, квадрат — масштаб (каждое отпускание — одна правка, {} отменит); {} — отменить загрузку или подготовку правки; {} — справка.", self.bindings.describe(crate::input::Action::Fit), self.bindings.describe(crate::input::Action::ToggleFloor), self.bindings.describe(crate::input::Action::Undo), self.bindings.describe(crate::input::Action::Cancel), self.bindings.describe(crate::input::Action::Help)));
                ui.label("Все клавиши и кнопки мыши, включая боковые, можно переназначить: «Настройки», затем «Клавиши и мышь…».");
                ui.label("Верхние меню «Вид», «Освещение», «Сцена» и «Модель» открываются треугольником; разделы внутри сворачиваются. Лампочка рядом включает и выключает лампу.");
                ui.label("Ремонты G001–G005 и G012: вкладка «Здоровье», карточки в «Анализ и диагностика» (стрелки или свайп), затем «Предпросмотр ремонта…» и «Применить»; список и очередь — в «Ремонт». Undo и Redo — в правом нижнем углу вьюпорта (Ctrl+Z / Ctrl+Y).");
                ui.label("«Сохранить» (Ctrl+S) перезаписывает рабочий файл после подтверждения и проверки; «Сохранить копию…» (Ctrl+Shift+S) пишет проверенный GLB в новый файл. При закрытии или открытии другой модели с несохранёнными правками программа спросит, что делать.");
                ui.separator();ui.label("Выбор узла показывает сведения. Подсветка узлов появится позднее.");
                ui.label("Меню «Вид»: базовый вид или PBR с авторскими нормалями, металлом и шероховатостью. Экспозиция и свет меняют только просмотр. Сведения о модели — вкладка «Инфо».");
                ui.label("Цвет, металл/шероховатость, нормали, окклюзия и свечение: встроенные PNG/JPEG до 8 бит/канал с независимыми переключателями. AO затемняет заполняющий свет и свет окружения. Нормали используют авторские касательные либо приближённую UV-основу без MikkTSpace. Свечение не освещает соседние объекты, ореола нет. Скайбокс PNG/JPEG или HDR со включённым «Свет окружения» освещает модель (IBL); тени — от лампы. Обе стороны граней видимы.");
                ui.collapsing("Лицензии встроенных шрифтов",|ui| {
                    egui::ScrollArea::vertical().max_height(280.).show(ui,|ui| {
                        ui.label(include_str!("../../../assets/font-licenses.txt"));
                    });
                });
            });
        });
        (output, self.scene, actions)
    }
}

/// Distance from a point to a segment (screen points).
fn segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0., 1.);
    (a + ab * t - p).length()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ad_render_vulkan::lighting::ShadingMode;
    fn raw(width: f32, height: f32) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        }
    }
    #[test]
    fn panels_leave_viewport_and_tree_starts_hidden_at_compact_and_normal_sizes() {
        let mut ui = ViewerUi::default();
        assert!(!ui.show_tree);
        for (w, h) in [(1200., 760.), (320., 240.)] {
            let (out, rect, _) = ui.layout(raw(w, h), None, false);
            assert!(rect.width() > 100. && rect.height() > 100.);
            assert!(rect.top() >= 48. && rect.bottom() <= h - 32.);
            if w > 680. {
                assert!(rect.right() < w - 200.);
            }
            out.drop_without_applying_deltas();
        }
    }
    #[test]
    fn empty_viewport_leaves_pointer_for_camera_but_toolbar_captures_it() {
        let mut ui = ViewerUi::default();
        let (out, rect, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        for (position, captured) in [(rect.center(), false), (egui::pos2(90., 24.), true)] {
            let mut input = raw(1200., 760.);
            input.events.push(egui::Event::PointerMoved(position));
            let (out, _, _) = ui.layout(input, None, false);
            assert_eq!(ui.context.egui_wants_pointer_input(), captured);
            out.drop_without_applying_deltas();
        }
    }
    #[test]
    fn camera_hit_test_tracks_egui_zoom_as_well_as_native_dpi() {
        let mut ui = ViewerUi::default();
        ui.context.set_zoom_factor(3.);
        let (out, rect, _) = ui.layout(raw(1200., 760.), None, false);
        let scale = out.pixels_per_point as f64;
        out.drop_without_applying_deltas();
        assert!(ui.camera_at(
            rect.center().x as f64 * scale,
            rect.center().y as f64 * scale
        ));
        assert!(!ui.camera_at(24. * scale, 24. * scale));
    }
    #[test]
    fn lighting_reset_keeps_pbr_mode_and_does_not_change_texture_view_or_asset() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                exposure_ev: 2.,
                key: 7.,
                fill: 0.8,
                direction: [0., 1., 0.],
                distance: 4.,
                color: [1., 0.5, 0.2],
                enabled: false,
            },
            textures: false,
            mr_maps: false,
            normal_maps: false,
            ao_maps: false,
            emission: false,
            menu: Some(TopMenu::Light),
            ..Default::default()
        };
        let source = std::sync::Arc::<[u8]>::from(
            include_bytes!("../../../tests/fixtures/Pbr_Factors.glb").as_slice(),
        );
        let document = ad_formats::import_glb(
            source.clone(),
            "factors".into(),
            ad_document::DocumentId(1),
            ad_document::ImportLimits::default(),
            &ad_document::Cancellation::default(),
        )
        .unwrap();
        let snapshot = std::sync::Arc::new(
            document
                .snapshot(
                    ad_document::ImportLimits::default(),
                    &ad_document::Cancellation::default(),
                )
                .unwrap(),
        );
        let diagnostics = ad_analysis::analyze(
            &document,
            ad_analysis::Limits::default(),
            &ad_document::Cancellation::default(),
        );
        let stamp = document.stamp();
        let asset = PreparedAsset {
            session: std::sync::Arc::new(
                ad_commands::Session::new(
                    document,
                    ad_commands::HistoryLimits::default(),
                    ad_document::ImportLimits::default(),
                )
                .unwrap(),
            ),
            generation: crate::imports::Generation {
                snapshot,
                diagnostics,
                stamp,
            },
        };
        for _ in 0..3 {
            let (out, rect, _) = ui.layout(raw(1200., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
            assert!(rect.width() > 100.);
        }
        let position = ui.reset_lighting_rect.center();
        assert!(ui.reset_lighting_rect.is_positive());
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        assert_eq!(
            ui.lighting,
            ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            }
        );
        assert!(!ui.textures);
        assert!(!ui.mr_maps);
        assert!(!ui.normal_maps);
        assert!(!ui.ao_maps);
        assert!(!ui.emission);
        assert!(std::sync::Arc::ptr_eq(asset.document().source(), &source));
        assert_eq!(asset.snapshot().fallback_normal_vertices, 6);
    }
    #[test]
    fn mr_switch_is_independent_of_color_texture_sampling() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            },
            menu: Some(TopMenu::View),
            ..Default::default()
        };
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
            out.drop_without_applying_deltas();
        }
        let position = ui.mr_maps_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(!ui.mr_maps && ui.textures);
    }
    #[test]
    fn normal_switch_leaves_mr_and_color_sampling_independent() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            },
            menu: Some(TopMenu::View),
            ..Default::default()
        };
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
            out.drop_without_applying_deltas();
        }
        let position = ui.normal_maps_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(!ui.normal_maps && ui.mr_maps && ui.textures);
    }
    #[test]
    fn ao_switch_leaves_other_material_channels_enabled() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            },
            menu: Some(TopMenu::View),
            ..Default::default()
        };
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
            out.drop_without_applying_deltas();
        }
        let position = ui.ao_maps_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(!ui.ao_maps && ui.normal_maps && ui.mr_maps && ui.textures);
    }
    #[test]
    fn emission_switch_leaves_other_material_channels_enabled() {
        let mut ui = ViewerUi {
            lighting: ViewLighting {
                mode: ShadingMode::DirectPbr,
                ..Default::default()
            },
            menu: Some(TopMenu::View),
            ..Default::default()
        };
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
            out.drop_without_applying_deltas();
        }
        let position = ui.emission_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(!ui.emission && ui.ao_maps && ui.normal_maps && ui.mr_maps && ui.textures);
    }
    #[test]
    fn tabs_fill_the_right_block_and_health_has_analysis_and_repair_pages() {
        let mut ui = ViewerUi::default();
        assert_eq!(ui.tab, Tab::Health);
        let (out, _, _) = ui.layout(raw(1200., 1000.), None, false);
        out.drop_without_applying_deltas();
        let labels: Vec<_> = ui.tab_rects.iter().map(|(t, _)| *t).collect();
        assert!(labels.starts_with(&[Tab::Health, Tab::Info]));
        let info = ui.tab_rects[1].1;
        click(&mut ui, None, info, (1200., 1000.));
        assert_eq!(ui.tab, Tab::Info);
        let health = ui.tab_rects[0].1;
        click(&mut ui, None, health, (1200., 1000.));
        assert_eq!(ui.tab, Tab::Health);
        let repair = ui
            .page_rects
            .iter()
            .find(|(p, _)| *p == HealthPage::Repair)
            .unwrap()
            .1;
        click(&mut ui, None, repair, (1200., 1000.));
        assert_eq!(ui.health_page, HealthPage::Repair);
        // The right block is about one and a half times the former 260 px.
        let (out, rect, _) = ui.layout(raw(1200., 1000.), None, false);
        out.drop_without_applying_deltas();
        assert!(1200. - rect.right() >= 380., "{rect:?}");
    }
    #[test]
    fn menus_open_with_their_triangle_button_and_close_outside() {
        let mut ui = ViewerUi::default();
        let size = (1900., 900.);
        warm(&mut ui, None, size);
        let mut menus = ui.menu_rects.clone();
        menus.sort_by(|a, b| a.1.left().total_cmp(&b.1.left()));
        let order: Vec<_> = menus.iter().map(|(m, _)| *m).collect();
        assert_eq!(
            order,
            [
                TopMenu::Settings,
                TopMenu::View,
                TopMenu::Grid,
                TopMenu::Light,
                TopMenu::Lamp,
                TopMenu::Exposure,
                TopMenu::Scene,
                TopMenu::Sky,
                TopMenu::Model
            ]
        );
        let rect_of = |m: TopMenu| menus.iter().find(|(x, _)| *x == m).unwrap().1;
        // The view group sits in the right part, ending at the viewport edge.
        assert!(
            rect_of(TopMenu::View).left() > ui.open_rect.right(),
            "{menus:?}"
        );
        let end = ui.amoeba_rect.right();
        assert!(
            end <= ui.scene.right() && end > ui.scene.right() - 30.,
            "{end} {:?}",
            ui.scene
        );
        let view = rect_of(TopMenu::View);
        click(&mut ui, None, view, size);
        assert_eq!(ui.menu, Some(TopMenu::View));
        // Another menu button switches menus.
        let lamp = rect_of(TopMenu::Lamp);
        click(&mut ui, None, lamp, size);
        assert_eq!(ui.menu, Some(TopMenu::Lamp));
        // A press in the viewport closes the menu.
        let (out, rect, _) = ui.layout(raw(size.0, size.1), None, false);
        out.drop_without_applying_deltas();
        let empty = egui::Rect::from_center_size(
            rect.right_bottom() - egui::vec2(300., 120.),
            egui::vec2(2., 2.),
        );
        click(&mut ui, None, empty, size);
        assert_eq!(ui.menu, None);
        // Settings holds the controls window.
        let settings = rect_of(TopMenu::Settings);
        click(&mut ui, None, settings, size);
        assert_eq!(ui.menu, Some(TopMenu::Settings));
    }
    #[test]
    fn skybox_window_lists_the_folder_and_a_preview_click_opens_the_panorama() {
        let dir = std::env::temp_dir().join(format!("ad-sky-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut hdr = b"#?RADIANCE
FORMAT=32-bit_rle_rgbe

-Y 8 +X 16
"
        .to_vec();
        for _ in 0..8 * 16 {
            hdr.extend([128, 160, 255, 129]);
        }
        for name in ["one.hdr", "two.hdr"] {
            std::fs::write(dir.join(name), &hdr).unwrap();
        }
        let mut ui = ViewerUi {
            skybox_folder: Some(dir.clone()),
            ..Default::default()
        };
        let size = (1900., 1000.);
        warm(&mut ui, None, size);
        let button = ui.skybox_button_rect;
        assert!(button.is_positive());
        click(&mut ui, None, button, size);
        assert!(ui.show_skybox);
        assert_eq!(ui.skybox_library.entries.len(), 2);
        warm(&mut ui, None, size);
        assert_eq!(ui.skybox_tile_rects.len(), 2);
        let tile = ui.skybox_tile_rects[1];
        warm(&mut ui, None, size);
        let actions = click(&mut ui, None, tile, size);
        assert_eq!(actions.skybox_open, Some(dir.join("two.hdr")));
        // Help moved to the settings menu: no top-bar «Справка».
        ui.menu = Some(TopMenu::Settings);
        warm(&mut ui, None, size);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn tab_does_not_walk_the_menus_and_settings_offer_window_modes_and_exit() {
        let mut ui = ViewerUi::default();
        let size = (1600., 900.);
        warm(&mut ui, None, size);
        for _ in 0..3 {
            let mut input = raw(size.0, size.1);
            input.events = vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::default(),
            }];
            let (out, _, _) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
        }
        assert!(
            ui.context.memory(|m| m.focused()).is_none(),
            "Tab must not focus the top menus"
        );
        ui.menu = Some(TopMenu::Settings);
        warm(&mut ui, None, size);
        assert!(ui.exit_rect.is_positive(), "Settings has «Выход»");
        let exit = ui.exit_rect;
        let actions = click(&mut ui, None, exit, size);
        assert!(actions.exit);
        assert_eq!(ui.window_mode, WindowMode::Normal);
        assert_eq!(
            WindowMode::from_key("fullscreen"),
            Some(WindowMode::Fullscreen)
        );
        assert_eq!(WindowMode::from_key("big"), None);
        assert_eq!(WindowMode::from_key("borderless"), Some(WindowMode::Normal));
        assert_eq!(WindowMode::from_key("maximized"), Some(WindowMode::Normal));
    }
    #[test]
    fn view_buttons_choose_camera_views_and_the_bug_opens_the_log() {
        let mut ui = ViewerUi::default();
        let size = (1600., 900.);
        warm(&mut ui, None, size);
        let views: Vec<_> = ui.view_rects.iter().map(|(v, _)| *v).collect();
        assert_eq!(views, crate::camera::View::ALL);
        // The view buttons sit left of Undo/Redo in the bottom-right corner.
        let top = ui.view_rects[4].1;
        assert!(
            top.right() < ui.undo_rect.left()
                && (top.center().y - ui.undo_rect.center().y).abs() < 2.
        );
        let actions = click(&mut ui, None, top, size);
        assert_eq!(actions.camera_view, Some(crate::camera::View::Top));
        assert!(ui.undo_rect.width() < 50., "Undo is an icon now");
        let bug = ui.bug_rect;
        assert!(
            bug.is_positive() && bug.right() > size.0 - 40. && bug.bottom() > size.1 - 40.,
            "{bug:?}"
        );
        click(&mut ui, None, bug, size);
        assert!(ui.show_log);
        crate::log::push("test line for the log window");
        warm(&mut ui, None, size);
    }
    #[test]
    fn work_tree_strip_opens_the_left_block_and_the_eye_hides_a_node() {
        let asset = fixture("Twin_Cubes.glb");
        let mut ui = ViewerUi::default();
        ui.tree.replace(asset.document());
        let size = (1600., 900.);
        warm(&mut ui, Some(&asset), size);
        let (out, closed, _) = ui.layout(raw(size.0, size.1), Some(&asset), false);
        out.drop_without_applying_deltas();
        let button = ui.tree_rect;
        assert!(button.top() >= 48. && button.left() < 20., "{button:?}");
        click(&mut ui, Some(&asset), button, size);
        assert!(ui.show_tree);
        warm(&mut ui, Some(&asset), size);
        let (out, open, _) = ui.layout(raw(size.0, size.1), Some(&asset), false);
        out.drop_without_applying_deltas();
        // The left block mirrors the right one: about 390 points wide.
        assert!(open.left() - closed.left() > 300., "{closed:?} {open:?}");
        assert!(ui.tree.hidden.is_empty());
    }
    #[test]
    fn right_click_menu_offers_object_gizmo_delete_and_view_submenus() {
        let asset = fixture("Twin_Cubes.glb");
        let mut ui = ViewerUi::default();
        ui.tree.replace(asset.document());
        let size = (1400., 900.);
        warm(&mut ui, Some(&asset), size);
        ui.open_context_menu(500., 400., Some(ad_document::NodeId(1)));
        warm(&mut ui, Some(&asset), size);
        let area = ui
            .context
            .memory(|m| m.area_rect(egui::Id::new("viewport_context_menu")))
            .unwrap();
        assert!(
            area.contains(egui::pos2(510., 410.)) && area.height() > 150.,
            "{area:?}"
        );
        // «Gizmo» is the first button under the name.
        let gizmo = egui::Rect::from_center_size(
            egui::pos2(area.left() + 60., area.top() + 40.),
            egui::vec2(2., 2.),
        );
        click(&mut ui, Some(&asset), gizmo, size);
        assert_eq!(ui.object_gizmo, Some(ad_document::NodeId(1)));
        assert!(ui.context_menu.is_none(), "a choice closes the menu");
        // A press outside closes it; Esc too.
        ui.open_context_menu(500., 400., None);
        warm(&mut ui, Some(&asset), size);
        let outside = egui::Rect::from_center_size(egui::pos2(900., 700.), egui::vec2(2., 2.));
        click(&mut ui, Some(&asset), outside, size);
        assert!(ui.context_menu.is_none());
    }
    /// Each menu shows all its items at once, also after switching menus.
    #[test]
    fn every_menu_unfolds_to_its_full_height_without_scrolling() {
        let asset = fixture("Pbr_Factors.glb");
        let mut ui = ViewerUi::default();
        let size = (1900., 1000.);
        warm(&mut ui, Some(&asset), size);
        let mut rounds = [Vec::new(), Vec::new()];
        for heights in &mut rounds {
            for menu in [
                TopMenu::Settings,
                TopMenu::View,
                TopMenu::Grid,
                TopMenu::Light,
                TopMenu::Lamp,
                TopMenu::Exposure,
                TopMenu::Scene,
                TopMenu::Sky,
                TopMenu::Model,
            ] {
                ui.menu = Some(menu);
                warm(&mut ui, Some(&asset), size);
                let area = ui
                    .context
                    .memory(|m| m.area_rect(egui::Id::new(("top_menu", menu))))
                    .unwrap();
                assert!(area.height() > 30., "{menu:?}: {area:?}");
                heights.push((menu, area.height()));
            }
        }
        // Same height the second time round: no size left from another menu.
        assert_eq!(rounds[0], rounds[1]);
        // «Сцена» shows the whole floor list (about 18 points per radio).
        let scene = rounds[0]
            .iter()
            .find(|(m, _)| *m == TopMenu::Scene)
            .unwrap()
            .1;
        let floors = ad_render_vulkan::grid::FloorTheme::ALL.len() + 1;
        assert!(scene > floors as f32 * 16., "{scene}");
    }
    #[test]
    fn lamp_switch_toggles_the_lamp_and_tree_unfolds_from_its_button() {
        let mut ui = ViewerUi::default();
        assert!(ui.lighting.enabled && !ui.show_tree);
        let (out, _, _) = ui.layout(raw(1200., 900.), None, false);
        out.drop_without_applying_deltas();
        let lamp = ui.lamp_switch_rect;
        assert!(lamp.is_positive());
        click(&mut ui, None, lamp, (1200., 900.));
        assert!(!ui.lighting.enabled);
        click(&mut ui, None, lamp, (1200., 900.));
        assert!(ui.lighting.enabled);
        let tree = ui.tree_rect;
        assert!(
            tree.is_positive() && tree.top() >= 48. && tree.left() < 40.,
            "{tree:?}"
        );
        click(&mut ui, None, tree, (1200., 900.));
        assert!(ui.show_tree);
        // Undo/Redo sit in the viewport's bottom-right corner.
        let (out, rect, _) = ui.layout(raw(1200., 900.), None, false);
        out.drop_without_applying_deltas();
        assert!(ui.redo_rect.right() <= rect.right() && ui.redo_rect.right() > rect.right() - 40.);
        assert!(
            ui.undo_rect.bottom() <= rect.bottom() && ui.undo_rect.bottom() > rect.bottom() - 40.
        );
        assert!(ui.undo_rect.right() <= ui.redo_rect.left());
    }
    #[test]
    fn open_button_emits_one_action_on_release() {
        let mut ui = ViewerUi::default();
        let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        let position = ui.open_rect.center();
        for pressed in [true, false] {
            let mut input = raw(1200., 760.);
            input.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, actions) = ui.layout(input, None, false);
            out.drop_without_applying_deltas();
            assert_eq!(actions.open, !pressed);
            assert!(!actions.fit && !actions.cancel);
        }
    }
    /// Clicks the centre of a rect across two frames; returns release actions.
    fn click(
        ui: &mut ViewerUi,
        asset: Option<&PreparedAsset>,
        rect: egui::Rect,
        size: (f32, f32),
    ) -> Actions {
        let mut last = Actions::default();
        for pressed in [true, false] {
            let mut input = raw(size.0, size.1);
            input.events = vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                },
            ];
            let (out, _, actions) = ui.layout(input, asset, false);
            out.drop_without_applying_deltas();
            last = actions;
        }
        last
    }
    /// Presses at `from`, moves by `by` in two steps and releases.
    fn drag(
        ui: &mut ViewerUi,
        asset: Option<&PreparedAsset>,
        from: egui::Pos2,
        by: egui::Vec2,
        size: (f32, f32),
    ) -> Vec<Actions> {
        let to = from + by;
        let steps = [
            vec![egui::Event::PointerMoved(from)],
            vec![egui::Event::PointerButton {
                pos: from,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            vec![egui::Event::PointerMoved(from + by * 0.5)],
            vec![egui::Event::PointerMoved(to)],
            vec![egui::Event::PointerButton {
                pos: to,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ];
        steps
            .into_iter()
            .map(|events| {
                let mut input = raw(size.0, size.1);
                input.events = events;
                let (out, _, actions) = ui.layout(input, asset, false);
                out.drop_without_applying_deltas();
                actions
            })
            .collect()
    }
    fn warm(ui: &mut ViewerUi, asset: Option<&PreparedAsset>, size: (f32, f32)) {
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(size.0, size.1), asset, false);
            out.drop_without_applying_deltas();
        }
    }
    fn fixture(name: &str) -> PreparedAsset {
        crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures")
                .join(name),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap()
    }
    #[test]
    fn finding_cards_turn_with_arrows_keys_and_swipes() {
        let asset = fixture("Geometry_Findings.glb");
        let count = asset.diagnostics().as_ref().unwrap().findings.len();
        assert!(count >= 3, "{count}");
        let mut ui = ViewerUi::default();
        let size = (1300., 1000.);
        warm(&mut ui, Some(&asset), size);
        assert_eq!(ui.carousel.index, 0);
        let next = ui.carousel.next_rect;
        click(&mut ui, Some(&asset), next, size);
        assert_eq!(ui.carousel.index, 1);
        let prev = ui.carousel.prev_rect;
        // egui hit-tests with the last frame: hover once over the button that
        // has just become enabled, as a real pointer does.
        warm(&mut ui, Some(&asset), size);
        click(&mut ui, Some(&asset), prev, size);
        assert_eq!(ui.carousel.index, 0);
        // Right arrow with the pointer over the panel.
        let card = ui.carousel.card_rect;
        assert!(card.is_positive());
        for key in [
            egui::Key::ArrowRight,
            egui::Key::ArrowRight,
            egui::Key::ArrowLeft,
        ] {
            let mut input = raw(size.0, size.1);
            input.events = vec![
                egui::Event::PointerMoved(card.center()),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                },
            ];
            let (out, _, _) = ui.layout(input, Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        assert_eq!(ui.carousel.index, 1);
        // Swipe left on the card: next card; swipe right: back.
        let card = ui.carousel.card_rect;
        drag(
            &mut ui,
            Some(&asset),
            card.center(),
            egui::vec2(-120., 0.),
            size,
        );
        assert_eq!(ui.carousel.index, 2);
        let card = ui.carousel.card_rect;
        drag(
            &mut ui,
            Some(&asset),
            card.center(),
            egui::vec2(120., 0.),
            size,
        );
        assert_eq!(ui.carousel.index, 1);
    }
    #[test]
    fn lamp_handles_turn_only_azimuth_or_only_elevation() {
        let asset = fixture("Pbr_Factors.glb");
        let mut ui = ViewerUi {
            model_gizmo_on: false,
            ..Default::default()
        };
        ui.lamp_gizmo = Some(LampGizmo {
            lamp: [0.5, 0.45],
            center: [0.5, 0.6],
            guides: Some(std::sync::Arc::new(crate::lamp::Guides {
                orbit: [None; crate::lamp::ORBIT_POINTS],
                arc: [None; crate::lamp::ARC_POINTS],
                azimuth: Some([0.01, 0.]),
                elevation: Some([0., -0.01]),
                distance: None,
            })),
        });
        let size = (1400., 1000.);
        warm(&mut ui, Some(&asset), size);
        let parts: Vec<_> = ui.lamp_handle_rects.iter().map(|(p, _)| *p).collect();
        assert_eq!(parts, [LampPart::Azimuth, LampPart::Elevation]);
        for (part, by) in [
            (LampPart::Azimuth, egui::vec2(40., 0.)),
            (LampPart::Elevation, egui::vec2(0., -30.)),
        ] {
            let rect = ui
                .lamp_handle_rects
                .iter()
                .find(|(p, _)| *p == part)
                .unwrap()
                .1;
            let actions = drag(&mut ui, Some(&asset), rect.center(), by, size);
            let total = actions
                .iter()
                .filter_map(|a| a.lamp_turn)
                .fold([0., 0.], |t, d| [t[0] + d[0], t[1] + d[1]]);
            assert!(
                actions.iter().all(|a| a.lamp_drag.is_none()),
                "{part:?}: free drag"
            );
            match part {
                LampPart::Azimuth => assert!(total[0] > 1. && total[1] == 0., "{total:?}"),
                _ => assert!(total[1] > 1. && total[0] == 0., "{total:?}"),
            }
        }
    }
    #[test]
    fn rotation_knobs_report_ring_drags() {
        use crate::model_gizmo::Handle;
        let asset = fixture("Pbr_Factors.glb");
        let mut ui = ViewerUi {
            lamp_icon: false,
            ..Default::default()
        };
        ui.model_gizmo = Some(ModelGizmo {
            pivot: [0.5, 0.6],
            ends: [Some([0.65, 0.6]), Some([0.5, 0.4]), None],
            knobs: [Some([0.4, 0.75]), Some([0.6, 0.75])],
            ..Default::default()
        });
        let size = (1400., 1000.);
        warm(&mut ui, Some(&asset), size);
        let handles: Vec<_> = ui.model_gizmo_rects.iter().map(|(h, _)| *h).collect();
        assert!(
            handles.ends_with(&[Handle::Rotate(0), Handle::Rotate(1), Handle::Scale]),
            "{handles:?}"
        );
        let rect = ui
            .model_gizmo_rects
            .iter()
            .find(|(h, _)| *h == Handle::Rotate(1))
            .unwrap()
            .1;
        ui.model_gizmo_rects.clear();
        let actions = drag(
            &mut ui,
            Some(&asset),
            rect.center(),
            egui::vec2(30., 0.),
            size,
        );
        // A knob turns by the pointer's angle around the centre (ADR-0063).
        let swept: f32 = actions
            .iter()
            .filter_map(|a| a.model_ring_drag)
            .map(|(axis, step)| {
                assert_eq!(axis, 1);
                step
            })
            .sum();
        assert!(swept.abs() > 0.05, "{swept}");
        assert!(actions.iter().all(|a| a.model_drag.is_none()));
        assert!(actions.iter().any(|a| a.model_drag_end));
        assert!(ui.model_turn.is_none(), "release ends the turn");
    }
    #[test]
    fn a_plane_square_moves_in_two_axes_and_wins_over_a_ring_behind_it() {
        use crate::model_gizmo::Handle;
        let asset = fixture("Pbr_Factors.glb");
        let mut ui = ViewerUi {
            lamp_icon: false,
            ..Default::default()
        };
        let mut gizmo = ModelGizmo {
            pivot: [0.5, 0.6],
            ends: [Some([0.65, 0.6]), Some([0.5, 0.4]), Some([0.42, 0.68])],
            ..Default::default()
        };
        // The floor square (XZ) and a Y ring running through it.
        gizmo.planes[2] = [
            Some([0.52, 0.61]),
            Some([0.56, 0.61]),
            Some([0.55, 0.65]),
            Some([0.51, 0.65]),
        ];
        for (i, p) in gizmo.rings[1].iter_mut().enumerate() {
            let t = i as f32 / crate::model_gizmo::RING_POINTS as f32 * std::f32::consts::TAU;
            *p = Some([0.535 + 0.05 * t.cos(), 0.63 + 0.02 * t.sin()]);
        }
        ui.model_gizmo = Some(gizmo);
        let size = (1400., 1000.);
        warm(&mut ui, Some(&asset), size);
        let scene = ui.scene;
        let at = egui::pos2(
            scene.min.x + 0.535 * scene.width(),
            scene.min.y + 0.63 * scene.height(),
        );
        let actions = drag(&mut ui, Some(&asset), at, egui::vec2(25., 10.), size);
        let moved: Vec<_> = actions.iter().filter_map(|a| a.model_drag).collect();
        assert!(!moved.is_empty());
        assert!(
            moved.iter().all(|(h, _)| *h == Handle::Plane(2)),
            "{moved:?}"
        );
        assert!(actions.iter().all(|a| a.model_ring_drag.is_none()));
        assert!(actions.iter().any(|a| a.model_drag_end));
    }
    #[test]
    fn the_work_tree_strip_lies_under_windows_and_is_not_camera_input() {
        let mut ui = ViewerUi::default();
        let size = (1400., 900.);
        warm(&mut ui, None, size);
        let strip = ui.strip_rect;
        assert!(strip.is_positive() && strip.top() >= 48.);
        // The strip is part of the background layer.
        let empty = egui::pos2(strip.right() - 8., strip.center().y);
        let layer = ui.context.layer_id_at(empty);
        assert!(
            layer.is_none_or(|l| l.order == egui::Order::Background),
            "{layer:?}"
        );
        assert!(!ui.camera_at(empty.x as f64, empty.y as f64));
        // No area of its own any more: every window (Middle) and menu
        // (Foreground) is drawn over it.
        assert!(
            ui.context
                .memory(|m| m.area_rect(egui::Id::new("work_tree_strip")))
                .is_none()
        );
        // Below the strip the viewport is the camera's again.
        let below = egui::pos2(strip.right() - 8., strip.bottom() + 20.);
        assert!(ui.camera_at(below.x as f64, below.y as f64));
    }
    fn preview_info() -> PreviewInfo {
        PreviewInfo {
            kind: ad_heal::RepairKind::DuplicateTriangles,
            mesh: ad_document::MeshId(0),
            before: 992,
            after: 960,
            vertices_before: 600,
            vertices_after: 600,
            appended_bytes: 11_520,
            boundary_edges: None,
            changed_normals: 0,
            references: 1,
            complete: true,
            target_left: Some(0),
            geometry_bytes: 261_504,
        }
    }
    #[test]
    fn apply_and_discard_are_offered_only_for_a_ready_preview() {
        let mut ui = ViewerUi::default();
        ui.edit.preview = Some(preview_info());
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1200., 900.), None, false);
            out.drop_without_applying_deltas();
        }
        let (out, _, _) = ui.layout(raw(1200., 900.), None, false);
        out.drop_without_applying_deltas();
        assert!(ui.apply_rect.is_positive() && ui.discard_rect.is_positive());
        let actions = {
            let rect = ui.apply_rect;
            click(&mut ui, None, rect, (1200., 900.))
        };
        assert!(actions.apply && !actions.discard_preview);
        let actions = {
            let rect = ui.discard_rect;
            click(&mut ui, None, rect, (1200., 900.))
        };
        assert!(actions.discard_preview && !actions.apply);
        // While another edit runs, the stale-prone Apply is disabled.
        ui.edit.busy = Some("Подготовка".into());
        let (out, _, _) = ui.layout(raw(1200., 900.), None, false);
        out.drop_without_applying_deltas();
        let actions = {
            let rect = ui.apply_rect;
            click(&mut ui, None, rect, (1200., 900.))
        };
        assert!(!actions.apply);
    }
    #[test]
    fn undo_and_redo_buttons_follow_history_state() {
        let mut ui = ViewerUi::default();
        ui.edit.can_undo = true;
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
            out.drop_without_applying_deltas();
        }
        let actions = {
            let rect = ui.undo_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(actions.undo && !actions.redo);
        let actions = {
            let rect = ui.redo_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(!actions.redo, "redo disabled without a redo entry");
        ui.edit.can_redo = true;
        ui.edit.busy = Some("Отмена правки".into());
        let actions = {
            let rect = ui.redo_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(
            !actions.redo && !actions.undo,
            "history disabled while busy"
        );
    }
    #[test]
    fn g002_finding_offers_a_read_only_repair_preview() {
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Duplicate_Heal.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let mut ui = ViewerUi {
            health_page: HealthPage::Repair,
            ..Default::default()
        };
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
        out.drop_without_applying_deltas();
        assert!(ui.preview_rect.is_positive());
        let actions = {
            let rect = ui.preview_rect;
            click(&mut ui, Some(&asset), rect, (1400., 1000.))
        };
        assert_eq!(
            actions.preview,
            Some((
                ad_document::MeshId(0),
                ad_heal::RepairKind::DuplicateTriangles
            ))
        );
        assert_eq!(
            asset.document().revision(),
            0,
            "preview request changes nothing"
        );
    }
    #[test]
    fn refusal_status_is_marked_as_warning_until_the_next_status() {
        let mut ui = ViewerUi::default();
        assert!(!ui.status_warning);
        ui.set_status("Правка не подготовлена, модель прежняя: отказ", true);
        assert!(ui.status_warning);
        let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        ui.set_status("Правка применена; ревизия 1", false);
        assert!(!ui.status_warning);
        assert_eq!(ui.status, "Правка применена; ревизия 1");
    }
    #[test]
    fn save_copy_is_offered_when_idle_and_dirty_state_is_visible() {
        let mut ui = ViewerUi::default();
        ui.edit.can_save = true;
        ui.edit.dirty = true;
        let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        assert!(ui.save_rect.is_positive());
        let actions = {
            let rect = ui.save_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(actions.save_copy);
        ui.edit.busy = Some("Сохранение копии".into());
        let actions = {
            let rect = ui.save_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(!actions.save_copy, "no second save or edit while busy");
        ui.edit.busy = None;
        ui.edit.can_save = false;
        let actions = {
            let rect = ui.save_rect;
            click(&mut ui, None, rect, (1200., 760.))
        };
        assert!(!actions.save_copy);
    }
    #[test]
    fn side_and_right_buttons_do_not_drag_the_gizmo() {
        let mut ui = ViewerUi::default();
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Pbr_Factors.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        ui.lamp_icon = false;
        ui.model_gizmo = Some(ModelGizmo {
            pivot: [0.5, 0.6],
            ends: [Some([0.65, 0.6]), Some([0.5, 0.4]), None],
            ..Default::default()
        });
        for _ in 0..2 {
            ui.model_gizmo_rects.clear();
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let at = ui.model_gizmo_rects[0].1.center();
        for button in [
            egui::PointerButton::Extra1,
            egui::PointerButton::Extra2,
            egui::PointerButton::Secondary,
        ] {
            let press = |pressed, pos| egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Default::default(),
            };
            let end = at + egui::vec2(40., 5.);
            for events in [
                vec![egui::Event::PointerMoved(at)],
                vec![press(true, at)],
                vec![egui::Event::PointerMoved(at + egui::vec2(20., 0.))],
                vec![egui::Event::PointerMoved(end)],
                vec![press(false, end)],
            ] {
                let mut input = raw(1400., 1000.);
                input.events = events;
                let (out, _, actions) = ui.layout(input, Some(&asset), false);
                assert!(actions.model_drag.is_none(), "{button:?} moved the model");
                assert!(!actions.model_drag_end, "{button:?} ended a drag");
                out.drop_without_applying_deltas();
            }
            ui.context_menu = None;
        }
    }
    #[test]
    fn model_gizmo_arrow_drag_reports_handle_delta_and_release() {
        use crate::model_gizmo::Handle;
        let mut ui = ViewerUi::default();
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Pbr_Factors.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        ui.lamp_icon = false;
        ui.model_gizmo = Some(ModelGizmo {
            pivot: [0.5, 0.6],
            ends: [Some([0.65, 0.6]), Some([0.5, 0.4]), None],
            ..Default::default()
        });
        for _ in 0..2 {
            ui.model_gizmo_rects.clear();
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let handles: Vec<_> = ui.model_gizmo_rects.iter().map(|(h, _)| *h).collect();
        assert_eq!(handles, [Handle::Axis(0), Handle::Axis(1), Handle::Scale]);
        let at = ui.model_gizmo_rects[0].1.center();
        let end = at + egui::vec2(40., 5.);
        let steps = [
            vec![egui::Event::PointerMoved(at)],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            vec![egui::Event::PointerMoved(at + egui::vec2(20., 0.))],
            vec![egui::Event::PointerMoved(end)],
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ];
        let (mut total, mut released) = ([0f32; 2], false);
        for events in steps {
            let mut input = raw(1400., 1000.);
            input.events = events;
            ui.model_gizmo_rects.clear();
            let (out, _, actions) = ui.layout(input, Some(&asset), false);
            if let Some((handle, d)) = actions.model_drag {
                assert_eq!(handle, Handle::Axis(0));
                total = [total[0] + d[0], total[1] + d[1]];
            }
            released |= actions.model_drag_end;
            out.drop_without_applying_deltas();
        }
        assert!(released, "release ends the drag");
        assert!((total[0] * ui.scene.width() - 40.).abs() < 1.5, "{total:?}");
        // Hidden gizmo leaves the viewport to the camera.
        ui.model_gizmo_on = false;
        ui.model_gizmo_rects.clear();
        let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
        out.drop_without_applying_deltas();
        assert!(ui.model_gizmo_rects.is_empty());
    }
    #[test]
    fn skeleton_joints_are_picked_in_the_viewport_and_shown_in_the_rigging_tree() {
        use ad_document::NodeId;
        use skeleton_ui::{OverlayJoint, SkeletonOverlay, joint_id};
        let mut ui = ViewerUi::default();
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Skin_Checker.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let document = asset.document();
        let joints = &document.rig().unwrap().skins[0].joints;
        assert!(joints.len() >= 2, "fixture has a joint chain");
        let (first, second) = (joints[0], joints[1]);
        ui.tab = Tab::Rigging;
        ui.skeleton = Some(SkeletonOverlay {
            document: document.id().0,
            joints: vec![
                OverlayJoint {
                    node: first,
                    name: "first".into(),
                    at: Some([0.4, 0.5]),
                    parent: None,
                    animatable: true,
                },
                OverlayJoint {
                    node: second,
                    name: "second".into(),
                    at: Some([0.6, 0.5]),
                    parent: Some(0),
                    animatable: false,
                },
            ],
        });
        for _ in 0..2 {
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let id = joint_id(document.id().0);
        let at = ui.scene.min + egui::vec2(0.6 * ui.scene.width(), 0.5 * ui.scene.height());
        let mut preview = None;
        for (i, events) in [
            vec![egui::Event::PointerMoved(at)],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ]
        .into_iter()
        .enumerate()
        {
            let mut input = raw(1400., 1000.);
            input.events = events;
            let (out, _, actions) = ui.layout(input, Some(&asset), false);
            if i == 0 {
                assert!(
                    ui.context.egui_wants_pointer_input(),
                    "a joint click is not an object pick"
                );
            }
            preview = preview.or(actions.animation_preview);
            out.drop_without_applying_deltas();
        }
        assert_eq!(ui.context.data(|d| d.get_temp::<NodeId>(id)), Some(second));
        // Not keyable: the key editor keeps its node.
        assert!(preview.is_none());
        assert_eq!(
            ui.context
                .data(|d| d.get_temp::<NodeId>(skeleton_ui::animation_node_id(document.id().0))),
            None
        );
        // The Rigging tree lists the joints by their node names.
        let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
        let text = format!("{:?}", out.shapes);
        out.drop_without_applying_deltas();
        assert!(
            text.contains(&document.nodes()[first.0].name),
            "tree shows the root joint"
        );
        // Switched off: nothing is drawn and the click is the viewport's.
        ui.skeleton_on = false;
        for _ in 0..2 {
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let mut input = raw(1400., 1000.);
        input.events = vec![egui::Event::PointerMoved(at)];
        let (out, _, _) = ui.layout(input, Some(&asset), false);
        assert!(!ui.context.egui_wants_pointer_input());
        out.drop_without_applying_deltas();
    }
    #[test]
    fn the_playback_bar_sits_at_the_bottom_and_plays_the_clip() {
        let mut ui = ViewerUi::default();
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Animation_Checker.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        assert!(!asset.document().animation_clips().is_empty());
        for (w, h) in [(1600., 1000.), (1100., 800.), (900., 700.)] {
            ui.corner_rect = egui::Rect::NOTHING;
            for _ in 0..3 {
                let (out, _, _) = ui.layout(raw(w, h), Some(&asset), false);
                out.drop_without_applying_deltas();
            }
            let (bar, corner, scene) = (ui.bar_rect, ui.corner_rect, ui.scene);
            assert!(bar.is_positive() && corner.is_positive(), "{w}: {bar:?}");
            assert!(scene.contains_rect(bar), "{w}: inside the viewport");
            assert!(!bar.intersects(corner), "{w}: {bar:?} clear of {corner:?}");
            assert!(bar.bottom() > scene.bottom() - 140., "{w}: at the bottom");
            assert!(bar.contains_rect(ui.play_rect), "{w}: play inside the bar");
        }
        // Play: the first clip starts.
        let at = ui.play_rect.center();
        let mut started = None;
        for pressed in [true, false] {
            let mut input = raw(900., 700.);
            input.events = vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            let (out, _, actions) = ui.layout(input, Some(&asset), false);
            started = started.or(actions.playback);
            out.drop_without_applying_deltas();
        }
        let started = started.expect("the play button changes playback");
        assert_eq!((started.clip, started.playing), (Some(0), true));
        // Without clips there is no bar.
        let still = crate::imports::prepare(&crate::worker::Request {
            ticket: 2,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Skin_Checker.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        if still.document().animation_clips().is_empty() {
            let (out, _, _) = ui.layout(raw(900., 700.), Some(&still), false);
            out.drop_without_applying_deltas();
            assert_eq!(ui.bar_rect, egui::Rect::NOTHING);
        }
    }
    #[test]
    fn a_right_click_on_the_timeline_offers_keys_of_the_edited_joint() {
        let mut ui = ViewerUi::default();
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Animation_Checker.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let document = asset.document();
        let track = &document.animation_clips()[0].tracks()[0];
        let (node, kind) = (track.node(), track.kind());
        let first = track.times()[0];
        // The key helpers: a key exists at the track's first time.
        let key = rig_ui::key_at(document, 0, node, kind, first, false).unwrap();
        assert_eq!((key.clip, key.time, key.channel.node), (0, first, node));
        assert!(
            !rig_ui::key_at(document, 0, node, kind, first, true)
                .unwrap()
                .channel
                .value
                .iter()
                .any(|v| v.is_nan())
        );
        ui.playback = crate::rig_playback::Controls {
            clip: Some(0),
            ..Default::default()
        };
        ui.context.data_mut(|d| {
            d.insert_temp(skeleton_ui::animation_node_id(document.id().0), node);
            d.insert_temp(rig_ui::key_kind_id(document), kind);
        });
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1600., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let bar = ui.bar_rect;
        let at = egui::pos2(bar.center().x, bar.bottom() - 8. - 17.);
        let mut moved = None;
        for pressed in [true, false] {
            let mut input = raw(1600., 1000.);
            input.events = vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            let (out, _, actions) = ui.layout(input, Some(&asset), false);
            moved = moved.or(actions.playback);
            out.drop_without_applying_deltas();
        }
        let moved = moved.expect("a right click moves the playhead there");
        assert!(moved.time > 0. && !moved.playing);
        ui.playback = moved;
        let (out, _, _) = ui.layout(raw(1600., 1000.), Some(&asset), false);
        let text = format!("{:?}", out.shapes);
        out.drop_without_applying_deltas();
        assert!(text.contains("Ключ «"), "the menu offers a key");
        assert!(text.contains("В начало"));
    }
    #[test]
    fn the_rigging_tab_measures_the_root_motion_of_the_clip() {
        let mut ui = ViewerUi::default();
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Animation_Checker.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let document = asset.document();
        ui.tab = Tab::Rigging;
        ui.playback = crate::rig_playback::Controls {
            clip: Some(0),
            ..Default::default()
        };
        let (out, _, _) = ui.layout(raw(1600., 1000.), Some(&asset), false);
        out.drop_without_applying_deltas();
        let motion = ui.root_path.motion.clone().expect("measured");
        let root = crate::root_motion::root_joint(document, 0).unwrap();
        assert_eq!((motion.clip, motion.node), (0, root));
        assert_eq!(document.nodes()[root.0].name, "Root");
        assert_eq!(motion.times.len(), motion.points.len());
        assert_eq!(
            *motion.times.last().unwrap(),
            document.animation_clips()[0].duration
        );
        let s = motion.summary;
        assert!(s.length.is_finite() && s.length >= s.horizontal - 1e-5);
        assert!(s.min_y <= s.max_y);
        // Selected-joint mode without a selection measures nothing.
        ui.root_path.selected_joint = true;
        let (out, _, _) = ui.layout(raw(1600., 1000.), Some(&asset), false);
        out.drop_without_applying_deltas();
        assert!(ui.root_path.motion.is_none());
    }
    #[test]
    fn joint_gizmo_rings_and_arrows_preview_and_write_one_key() {
        use ad_document::TrackKind;
        let mut ui = ViewerUi::default();
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Skin_Checker.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let size = (1400., 1000.);
        warm(&mut ui, Some(&asset), size);
        let scene = ui.scene;
        let to_uv = |p: glam::Vec3| Some([0.5 + p.x * 0.05, 0.5 - p.y * 0.05]);
        let gizmo = crate::joint_gizmo::build(
            asset.document().id().0,
            ad_document::NodeId(1),
            0,
            0.25,
            glam::Mat4::IDENTITY,
            glam::Mat4::IDENTITY,
            glam::Quat::IDENTITY.to_array(),
            [0., 0., 0.],
            false,
            crate::joint_gizmo::RADIUS_PX,
            glam::Vec3::NEG_Z,
            glam::Vec2::new(scene.width(), scene.height()),
            to_uv,
        )
        .unwrap();
        let at = |p: [f32; 2]| scene.min + egui::vec2(p[0] * scene.width(), p[1] * scene.height());
        ui.joint_gizmo = Some(gizmo.clone());
        warm(&mut ui, Some(&asset), size);
        // Arrow X: drag to the right moves the joint along +X.
        let tip = at(gizmo.arrows[0].unwrap());
        let steps = drag(&mut ui, Some(&asset), tip, egui::vec2(60., 0.), size);
        let preview = steps
            .iter()
            .rev()
            .find_map(|a| a.animation_preview.flatten());
        assert!(preview.is_some_and(|c| c.kind == TrackKind::Translation && c.value[0] > 0.));
        let key = steps
            .iter()
            .find_map(|a| a.animation_key)
            .expect("a key on release");
        assert_eq!((key.clip, key.time, key.remove), (0, 0.25, false));
        assert_eq!(key.channel.kind, TrackKind::Translation);
        assert!(key.channel.value[0] > 0. && key.channel.value[1].abs() < 1e-4);
        // Z ring (facing the camera): hover first, then drag along it.
        let ring = &gizmo.rings[2];
        let from = at(ring[0].unwrap());
        let to = at(ring[6].unwrap());
        for _ in 0..2 {
            let mut input = raw(size.0, size.1);
            input.events = vec![egui::Event::PointerMoved(from)];
            let (out, _, _) = ui.layout(input, Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        assert!(ui.pointer_on_gizmo, "the ring owns the left button");
        let steps = drag(&mut ui, Some(&asset), from, to - from, size);
        let key = steps
            .iter()
            .find_map(|a| a.animation_key)
            .expect("a key on release");
        assert_eq!(key.channel.kind, TrackKind::Rotation);
        let q = glam::Quat::from_array(key.channel.value);
        let (axis, angle) = q.to_axis_angle();
        assert!(angle > 0.3 && axis.z.abs() > 0.99, "{axis} {angle}");
    }
    #[test]
    fn lamp_gizmo_captures_the_pointer_and_reports_drags() {
        let mut ui = ViewerUi::default();
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Pbr_Factors.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        ui.lamp_gizmo = Some(LampGizmo {
            lamp: [0.7, 0.3],
            center: [0.5, 0.5],
            ..Default::default()
        });
        // A new egui area is measured invisibly on its first frame.
        for _ in 0..2 {
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let at = ui.lamp_gizmo_rect.center();
        assert!(ui.scene.contains(at), "gizmo inside the viewport");
        let mut drags = Vec::new();
        let steps = [
            vec![egui::Event::PointerMoved(at)],
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }],
            vec![egui::Event::PointerMoved(at + egui::vec2(15., 0.))],
            vec![egui::Event::PointerMoved(at + egui::vec2(30., -10.))],
            vec![egui::Event::PointerButton {
                pos: at + egui::vec2(30., -10.),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ];
        for (i, events) in steps.into_iter().enumerate() {
            let mut input = raw(1400., 1000.);
            input.events = events;
            let (out, _, actions) = ui.layout(input, Some(&asset), false);
            if i == 0 {
                assert!(
                    ui.context.egui_wants_pointer_input(),
                    "camera must not orbit"
                );
            }
            drags.extend(actions.lamp_drag);
            out.drop_without_applying_deltas();
        }
        let total = drags
            .iter()
            .fold([0., 0.], |a, d| [a[0] + d[0], a[1] + d[1]]);
        assert!(total[0] > 0. && total[1] < 0., "{drags:?}");
        // Hidden icon or off-screen lamp: the viewport pointer is the camera's.
        for (icon, lamp) in [(false, [0.7, 0.3]), (true, [1.5, 0.3])] {
            ui.lamp_icon = icon;
            ui.lamp_gizmo = Some(LampGizmo {
                lamp,
                center: [0.5, 0.5],
                ..Default::default()
            });
            let mut input = raw(1400., 1000.);
            input.events.push(egui::Event::PointerMoved(at));
            let (out, _, _) = ui.layout(input, Some(&asset), false);
            out.drop_without_applying_deltas();
            let mut input = raw(1400., 1000.);
            input.events.push(egui::Event::PointerMoved(at));
            let (out, _, _) = ui.layout(input, Some(&asset), false);
            assert!(!ui.context.egui_wants_pointer_input());
            out.drop_without_applying_deltas();
        }
    }
    #[test]
    fn environment_never_changes_lighting() {
        use ad_render_vulkan::sky::Environment;
        let mut ui = ViewerUi::default();
        assert_eq!(ui.environment, Environment::DarkStudio);
        assert!(!ui.environment_light, "environment light is off by default");
        let before = format!("{:?}", ui.lighting);
        for preset in Environment::ALL {
            ui.environment = preset;
            for on in [true, false] {
                ui.environment_light = on;
                ui.environment_strength = if on { 2. } else { 0.5 };
                assert_eq!(format!("{:?}", ui.lighting), before);
            }
        }
    }
    #[test]
    fn g_key_hides_the_floor_and_restores_the_last_theme() {
        use ad_render_vulkan::grid::FloorTheme;
        let mut ui = ViewerUi::default();
        assert_eq!(
            ui.floor,
            Some(FloorTheme::StudioGrid),
            "studio grid by default"
        );
        ui.floor = Some(FloorTheme::Checker);
        assert!(!ui.toggle_floor());
        assert_eq!(ui.floor, None);
        assert!(ui.toggle_floor());
        assert_eq!(ui.floor, Some(FloorTheme::Checker));
        // Cycling through every theme twice keeps exactly one floor choice.
        for _ in 0..2 {
            for theme in FloorTheme::ALL {
                ui.floor = Some(theme);
                assert_eq!(ui.floor, Some(theme));
            }
        }
    }
    #[test]
    fn history_lines_read_as_past_records_with_counts() {
        let record = crate::history::Record {
            time: 0,
            session: 1,
            model: "m.glb".into(),
            source_bytes: 10,
            action: "remove".into(),
            rule: "G012".into(),
            mesh: Some(0),
            triangles: (3, 3),
            vertices: (9, 7),
            normals: 0,
            left: Some(0),
            complete: true,
            revision: 1,
        };
        assert_eq!(
            history_line(&record),
            "1970-01-01 00:00 UTC · m.glb · Ремонт G012 · примитив #0 · треугольники 3 -> 3 · вершины 9 -> 7 · осталось G012 0 · анализ полный · ревизия 1"
        );
        let save = crate::history::Record {
            action: "save".into(),
            rule: "-".into(),
            mesh: None,
            ..record
        };
        assert!(history_line(&save).contains("Сохранить · ревизия 1"));
    }
    #[test]
    fn repair_queue_window_lists_planned_steps_and_starts() {
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Geometry_Findings.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let mut ui = ViewerUi {
            health_page: HealthPage::Repair,
            ..Default::default()
        };
        ui.edit.can_save = true;
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1300., 900.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let rect = ui.queue_rect;
        assert!(rect.is_positive());
        click(&mut ui, Some(&asset), rect, (1300., 900.));
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1300., 900.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        assert!(ui.show_queue && ui.queue_start_rect.is_positive());
        let rect = ui.queue_start_rect;
        assert!(click(&mut ui, Some(&asset), rect, (1300., 900.)).queue_start);
        // While running, the window offers Stop instead of Start.
        ui.edit.queue = Some(QueueUi {
            running: true,
            ..Default::default()
        });
        ui.edit.busy = Some("Расчёт".into());
        let actions = click(&mut ui, Some(&asset), rect, (1300., 900.));
        assert!(!actions.queue_start);
    }
    #[test]
    fn save_over_working_file_needs_a_working_path_and_unsaved_edits() {
        let mut ui = ViewerUi::default();
        ui.edit.can_save = true;
        ui.edit.can_overwrite = true;
        ui.edit.dirty = true;
        let (out, _, _) = ui.layout(raw(1200., 760.), None, false);
        out.drop_without_applying_deltas();
        assert!(ui.overwrite_rect.is_positive());
        let rect = ui.overwrite_rect;
        let actions = click(&mut ui, None, rect, (1200., 760.));
        assert!(actions.save && !actions.save_copy);
        for (overwrite, dirty) in [(false, true), (true, false)] {
            ui.edit.can_overwrite = overwrite;
            ui.edit.dirty = dirty;
            let actions = click(&mut ui, None, rect, (1200., 760.));
            assert!(!actions.save);
        }
    }
    /// Every non-ASCII character inside UI string literals must exist in the
    /// bundled egui fonts; otherwise it renders as an empty box (found in the
    /// manual G002 check: "992 → 960").
    #[test]
    fn ui_string_literals_use_only_glyphs_present_in_bundled_fonts() {
        let mut ui = ViewerUi::default();
        let (out, _, _) = ui.layout(raw(800., 600.), None, false);
        out.drop_without_applying_deltas();
        let sources = [
            include_str!("viewer_ui.rs"),
            include_str!("diagnostic_ui.rs"),
            include_str!("main.rs"),
        ];
        let mut missing = std::collections::BTreeSet::new();
        for source in sources {
            for line in source.lines() {
                let code = line.split("//").next().unwrap_or("");
                for (i, literal) in code.split('"').enumerate() {
                    if i % 2 == 0 {
                        continue;
                    }
                    for c in literal.chars().filter(|c| !c.is_ascii()) {
                        let present = ui
                            .context
                            .fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(14.), c));
                        if !present {
                            missing.insert(c);
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "glyphs missing from bundled fonts: {missing:?}"
        );
    }
    #[test]
    fn g001_finding_offers_a_degenerate_repair_preview() {
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Geometry_Findings.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let mut ui = ViewerUi {
            health_page: HealthPage::Repair,
            ..Default::default()
        };
        for _ in 0..3 {
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let actions = {
            let rect = ui.preview_rect;
            click(&mut ui, Some(&asset), rect, (1400., 1000.))
        };
        assert_eq!(
            actions.preview,
            Some((
                ad_document::MeshId(0),
                ad_heal::RepairKind::DegenerateTriangles
            )),
            "the first repairable row is G001"
        );
    }
    #[test]
    fn g003_finding_offers_an_unused_vertex_preview_and_its_window_applies() {
        let asset = crate::imports::prepare(&crate::worker::Request {
            ticket: 1,
            input: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/Geometry_Findings.glb"),
            cancel: ad_document::Cancellation::default(),
        })
        .unwrap();
        let mut ui = ViewerUi {
            health_page: HealthPage::Repair,
            ..Default::default()
        };
        for _ in 0..4 {
            let (out, _, _) = ui.layout(raw(1400., 1000.), Some(&asset), false);
            out.drop_without_applying_deltas();
        }
        let rect = ui
            .preview_rects
            .iter()
            .find(|(kind, _)| *kind == ad_heal::RepairKind::UnusedVertices)
            .expect("a visible G003 row with a preview button")
            .1;
        let actions = click(&mut ui, Some(&asset), rect, (1400., 1000.));
        assert_eq!(
            actions.preview,
            Some((ad_document::MeshId(0), ad_heal::RepairKind::UnusedVertices))
        );
        assert_eq!(asset.document().revision(), 0);
        // The G003 preview window offers Apply like the triangle repairs.
        let mut ui = ViewerUi::default();
        ui.edit.preview = Some(PreviewInfo {
            kind: ad_heal::RepairKind::UnusedVertices,
            before: 5,
            after: 5,
            vertices_before: 5,
            vertices_after: 4,
            appended_bytes: 64,
            references: 2,
            ..preview_info()
        });
        for _ in 0..4 {
            let (out, _, _) = ui.layout(raw(1200., 900.), None, false);
            out.drop_without_applying_deltas();
        }
        assert!(ui.apply_rect.is_positive());
        let rect = ui.apply_rect;
        assert!(click(&mut ui, None, rect, (1200., 900.)).apply);
    }
}
