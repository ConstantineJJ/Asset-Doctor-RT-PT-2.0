//! ADR-0043 input bindings: every keyboard and mouse action of the viewer has
//! up to two chords the owner can change. Conflicts are refused unless the
//! owner chooses to move the chord; defaults and reset are always available.
//! Saved as readable JSON next to the repair history; never part of a model.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use winit::{event::MouseButton, keyboard::KeyCode};

/// What a chord does. Commands fire on press; drags act while held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    Open,
    Save,
    SaveCopy,
    Undo,
    Redo,
    Fit,
    ToggleFloor,
    ToggleWireframe,
    ToggleAxes,
    ToggleBounds,
    ToggleLampIcon,
    ToggleModelGizmo,
    ToggleOutline,
    DeleteObject,
    ToggleLeftPanel,
    ToggleRightPanel,
    ToggleFullscreen,
    ToggleFrameless,
    ToggleSkeleton,
    Grab,
    Help,
    Cancel,
    Orbit,
    Pan,
}
impl Action {
    pub const ALL: [Self; 24] = [
        Self::Open,
        Self::Save,
        Self::SaveCopy,
        Self::Undo,
        Self::Redo,
        Self::Fit,
        Self::ToggleFloor,
        Self::ToggleWireframe,
        Self::ToggleAxes,
        Self::ToggleBounds,
        Self::ToggleLampIcon,
        Self::ToggleModelGizmo,
        Self::ToggleOutline,
        Self::DeleteObject,
        Self::ToggleLeftPanel,
        Self::ToggleRightPanel,
        Self::ToggleFullscreen,
        Self::ToggleFrameless,
        Self::ToggleSkeleton,
        Self::Grab,
        Self::Help,
        Self::Cancel,
        Self::Orbit,
        Self::Pan,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Открыть модель",
            Self::Save => "Сохранить",
            Self::SaveCopy => "Сохранить копию",
            Self::Undo => "Отменить правку",
            Self::Redo => "Повторить правку",
            Self::Fit => "Вписать модель",
            Self::ToggleFloor => "Пол: скрыть/вернуть",
            Self::ToggleWireframe => "Каркас: вкл/выкл",
            Self::ToggleAxes => "Оси: вкл/выкл",
            Self::ToggleBounds => "Габариты: вкл/выкл",
            Self::ToggleLampIcon => "Значок лампы: вкл/выкл",
            Self::ToggleModelGizmo => "Гизмо модели: вкл/выкл",
            Self::ToggleOutline => "Контур объектов: вкл/выкл",
            Self::DeleteObject => "Удалить выбранные объекты",
            Self::ToggleLeftPanel => "Левая панель (Work-Tree): скрыть/показать",
            Self::ToggleRightPanel => "Правая панель: скрыть/показать",
            Self::ToggleFullscreen => "Полный экран: вкл/выкл",
            Self::ToggleFrameless => "Окно без рамки и заголовка: вкл/выкл",
            Self::ToggleSkeleton => "Риг (скелет): показать/скрыть",
            Self::Grab => "Захват: взять / положить выбранное",
            Self::Help => "Справка",
            Self::Cancel => "Отмена загрузки/правки",
            Self::Orbit => "Камера: вращать (держать)",
            Self::Pan => "Камера: сдвигать (держать)",
        }
    }
    /// Stable name in the bindings file.
    pub fn key(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Save => "save",
            Self::SaveCopy => "save_copy",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Fit => "fit",
            Self::ToggleFloor => "toggle_floor",
            Self::ToggleWireframe => "toggle_wireframe",
            Self::ToggleAxes => "toggle_axes",
            Self::ToggleBounds => "toggle_bounds",
            Self::ToggleLampIcon => "toggle_lamp_icon",
            Self::ToggleModelGizmo => "toggle_model_gizmo",
            Self::ToggleOutline => "toggle_outline",
            Self::DeleteObject => "delete_object",
            Self::ToggleLeftPanel => "toggle_left_panel",
            Self::ToggleRightPanel => "toggle_right_panel",
            Self::ToggleFullscreen => "toggle_fullscreen",
            Self::ToggleFrameless => "toggle_frameless",
            Self::ToggleSkeleton => "toggle_skeleton",
            Self::Grab => "grab",
            Self::Help => "help",
            Self::Cancel => "cancel",
            Self::Orbit => "orbit",
            Self::Pan => "pan",
        }
    }
    /// Held mouse actions; everything else fires once on press.
    pub fn is_drag(self) -> bool {
        matches!(self, Self::Orbit | Self::Pan)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}
impl Mods {
    pub const NONE: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// One chord: a key or mouse button with modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub input: Input,
    pub mods: Mods,
}
impl Chord {
    pub fn key(key: KeyCode, mods: Mods) -> Self {
        Self {
            input: Input::Key(key),
            mods,
        }
    }
    pub fn mouse(button: MouseButton) -> Self {
        Self {
            input: Input::Mouse(button),
            mods: Mods::NONE,
        }
    }
    fn ctrl(key: KeyCode) -> Self {
        Self::key(
            key,
            Mods {
                ctrl: true,
                ..Mods::NONE
            },
        )
    }
    fn plain(key: KeyCode) -> Self {
        Self::key(key, Mods::NONE)
    }
    /// Readable form, also the file format: "Ctrl+Shift+S", "Mouse Back".
    pub fn name(self) -> String {
        let mut text = String::new();
        for (on, name) in [
            (self.mods.ctrl, "Ctrl+"),
            (self.mods.shift, "Shift+"),
            (self.mods.alt, "Alt+"),
        ] {
            if on {
                text.push_str(name);
            }
        }
        match self.input {
            Input::Key(key) => text.push_str(key_name(key).unwrap_or("?")),
            Input::Mouse(button) => text.push_str(&mouse_name(button)),
        }
        text
    }
    pub fn parse(text: &str) -> Option<Self> {
        let mut mods = Mods::NONE;
        let mut rest = text.trim();
        loop {
            if let Some(r) = rest.strip_prefix("Ctrl+") {
                mods.ctrl = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Shift+") {
                mods.shift = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Alt+") {
                mods.alt = true;
                rest = r;
            } else {
                break;
            }
        }
        let input = if let Some(m) = rest.strip_prefix("Mouse ") {
            Input::Mouse(parse_mouse(m)?)
        } else {
            Input::Key(KEYS.iter().find(|(_, n)| *n == rest)?.0)
        };
        Some(Self { input, mods })
    }
}

/// Keys that can be bound, with their names (physical positions, US labels).
const KEYS: &[(KeyCode, &str)] = &[
    (KeyCode::KeyA, "A"),
    (KeyCode::KeyB, "B"),
    (KeyCode::KeyC, "C"),
    (KeyCode::KeyD, "D"),
    (KeyCode::KeyE, "E"),
    (KeyCode::KeyF, "F"),
    (KeyCode::KeyG, "G"),
    (KeyCode::KeyH, "H"),
    (KeyCode::KeyI, "I"),
    (KeyCode::KeyJ, "J"),
    (KeyCode::KeyK, "K"),
    (KeyCode::KeyL, "L"),
    (KeyCode::KeyM, "M"),
    (KeyCode::KeyN, "N"),
    (KeyCode::KeyO, "O"),
    (KeyCode::KeyP, "P"),
    (KeyCode::KeyQ, "Q"),
    (KeyCode::KeyR, "R"),
    (KeyCode::KeyS, "S"),
    (KeyCode::KeyT, "T"),
    (KeyCode::KeyU, "U"),
    (KeyCode::KeyV, "V"),
    (KeyCode::KeyW, "W"),
    (KeyCode::KeyX, "X"),
    (KeyCode::KeyY, "Y"),
    (KeyCode::KeyZ, "Z"),
    (KeyCode::Digit0, "0"),
    (KeyCode::Digit1, "1"),
    (KeyCode::Digit2, "2"),
    (KeyCode::Digit3, "3"),
    (KeyCode::Digit4, "4"),
    (KeyCode::Digit5, "5"),
    (KeyCode::Digit6, "6"),
    (KeyCode::Digit7, "7"),
    (KeyCode::Digit8, "8"),
    (KeyCode::Digit9, "9"),
    (KeyCode::F1, "F1"),
    (KeyCode::F2, "F2"),
    (KeyCode::F3, "F3"),
    (KeyCode::F4, "F4"),
    (KeyCode::F5, "F5"),
    (KeyCode::F6, "F6"),
    (KeyCode::F7, "F7"),
    (KeyCode::F8, "F8"),
    (KeyCode::F9, "F9"),
    (KeyCode::F10, "F10"),
    (KeyCode::F11, "F11"),
    (KeyCode::F12, "F12"),
    (KeyCode::Escape, "Esc"),
    (KeyCode::Space, "Space"),
    (KeyCode::Tab, "Tab"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::Delete, "Delete"),
    (KeyCode::Insert, "Insert"),
    (KeyCode::Home, "Home"),
    (KeyCode::End, "End"),
    (KeyCode::PageUp, "PageUp"),
    (KeyCode::PageDown, "PageDown"),
    (KeyCode::ArrowUp, "Up"),
    (KeyCode::ArrowDown, "Down"),
    (KeyCode::ArrowLeft, "Left"),
    (KeyCode::ArrowRight, "Right"),
    (KeyCode::Backquote, "`"),
    (KeyCode::Minus, "-"),
    (KeyCode::Equal, "="),
    (KeyCode::BracketLeft, "["),
    (KeyCode::BracketRight, "]"),
    (KeyCode::Backslash, "\\"),
    (KeyCode::Semicolon, ";"),
    (KeyCode::Quote, "'"),
    (KeyCode::Comma, ","),
    (KeyCode::Period, "."),
    (KeyCode::Slash, "/"),
    (KeyCode::Numpad0, "Num0"),
    (KeyCode::Numpad1, "Num1"),
    (KeyCode::Numpad2, "Num2"),
    (KeyCode::Numpad3, "Num3"),
    (KeyCode::Numpad4, "Num4"),
    (KeyCode::Numpad5, "Num5"),
    (KeyCode::Numpad6, "Num6"),
    (KeyCode::Numpad7, "Num7"),
    (KeyCode::Numpad8, "Num8"),
    (KeyCode::Numpad9, "Num9"),
    (KeyCode::NumpadAdd, "Num+"),
    (KeyCode::NumpadSubtract, "Num-"),
];
pub fn key_name(key: KeyCode) -> Option<&'static str> {
    KEYS.iter().find(|(k, _)| *k == key).map(|(_, n)| *n)
}
/// Modifier keys alone are never a chord; capture waits for the main key.
pub fn is_modifier(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight
    )
}
fn mouse_name(button: MouseButton) -> String {
    match button {
        MouseButton::Left => "Mouse Left".into(),
        MouseButton::Right => "Mouse Right".into(),
        MouseButton::Middle => "Mouse Middle".into(),
        MouseButton::Back => "Mouse Back".into(),
        MouseButton::Forward => "Mouse Forward".into(),
        MouseButton::Other(n) => format!("Mouse {n}"),
    }
}
fn parse_mouse(text: &str) -> Option<MouseButton> {
    Some(match text {
        "Left" => MouseButton::Left,
        "Right" => MouseButton::Right,
        "Middle" => MouseButton::Middle,
        "Back" => MouseButton::Back,
        "Forward" => MouseButton::Forward,
        n => MouseButton::Other(n.parse().ok()?),
    })
}

/// Up to two chords per action.
pub type Slots = [Option<Chord>; 2];

#[derive(Clone, Debug, PartialEq)]
pub struct Bindings {
    slots: Vec<(Action, Slots)>,
    /// Scroll up zooms out instead of in.
    pub invert_zoom: bool,
    /// Read from an older file and changed on the way (save it back).
    pub migrated: bool,
}
impl Default for Bindings {
    fn default() -> Self {
        let slots = Action::ALL.iter().map(|&a| (a, default_slots(a))).collect();
        Self {
            slots,
            invert_zoom: false,
            migrated: false,
        }
    }
}
fn shift_mouse(button: MouseButton) -> Chord {
    Chord {
        input: Input::Mouse(button),
        mods: Mods {
            shift: true,
            ..Mods::NONE
        },
    }
}
pub fn default_slots(action: Action) -> Slots {
    use KeyCode as K;
    let shift_ctrl = |key| {
        Chord::key(
            key,
            Mods {
                ctrl: true,
                shift: true,
                alt: false,
            },
        )
    };
    match action {
        Action::Open => [Some(Chord::ctrl(K::KeyO)), None],
        Action::Save => [Some(Chord::ctrl(K::KeyS)), None],
        Action::SaveCopy => [Some(shift_ctrl(K::KeyS)), None],
        Action::Undo => [Some(Chord::ctrl(K::KeyZ)), None],
        Action::Redo => [Some(Chord::ctrl(K::KeyY)), Some(shift_ctrl(K::KeyZ))],
        // 2026-11-25: the middle button grabs objects; F still frames.
        Action::Fit => [Some(Chord::plain(K::KeyF)), None],
        Action::Grab => [Some(Chord::mouse(MouseButton::Middle)), None],
        Action::ToggleOutline => [Some(Chord::plain(K::Tab)), None],
        Action::DeleteObject => [Some(Chord::plain(K::KeyX)), Some(Chord::plain(K::Delete))],
        Action::ToggleLeftPanel => [Some(Chord::plain(K::KeyQ)), None],
        Action::ToggleRightPanel => [Some(Chord::plain(K::KeyE)), None],
        Action::ToggleFullscreen => [Some(Chord::plain(K::F11)), None],
        Action::ToggleSkeleton => [Some(Chord::plain(K::KeyZ)), None],
        Action::ToggleFrameless => [Some(Chord::plain(K::F1)), None],
        Action::ToggleFloor => [Some(Chord::plain(K::KeyG)), None],
        Action::Help => [Some(Chord::plain(K::KeyH)), None],
        Action::Cancel => [Some(Chord::plain(K::Escape)), None],
        // 2026-11-21: the left button selects and the right one opens the
        // context menu; the side buttons turn and move the camera, with
        // Shift + left / right as the fallback for mice without them.
        Action::Orbit => [
            Some(Chord::mouse(MouseButton::Forward)),
            Some(shift_mouse(MouseButton::Left)),
        ],
        Action::Pan => [
            Some(Chord::mouse(MouseButton::Back)),
            Some(shift_mouse(MouseButton::Right)),
        ],
        _ => [None, None],
    }
}

/// Assignment refused: the chord already belongs to another action/slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub action: Action,
    pub slot: usize,
}

impl Bindings {
    pub fn slots(&self, action: Action) -> Slots {
        self.slots
            .iter()
            .find(|(a, _)| *a == action)
            .map_or([None, None], |(_, s)| *s)
    }
    fn slots_mut(&mut self, action: Action) -> &mut Slots {
        let index = self
            .slots
            .iter()
            .position(|(a, _)| *a == action)
            .expect("every action has slots");
        &mut self.slots[index].1
    }
    /// Who owns `chord` now, if anyone.
    pub fn owner(&self, chord: Chord) -> Option<Conflict> {
        self.slots.iter().find_map(|(action, slots)| {
            slots
                .iter()
                .position(|s| *s == Some(chord))
                .map(|slot| Conflict {
                    action: *action,
                    slot,
                })
        })
    }
    /// Sets one slot. A chord used elsewhere is refused with its owner; the
    /// same slot keeps it. `replace` moves it from the owner instead.
    pub fn assign(
        &mut self,
        action: Action,
        slot: usize,
        chord: Chord,
        replace: bool,
    ) -> Result<(), Conflict> {
        let slot = slot.min(1);
        if let Some(owner) = self.owner(chord)
            && (owner.action, owner.slot) != (action, slot)
        {
            if !replace {
                return Err(owner);
            }
            self.slots_mut(owner.action)[owner.slot] = None;
        }
        self.slots_mut(action)[slot] = Some(chord);
        Ok(())
    }
    pub fn clear(&mut self, action: Action, slot: usize) {
        self.slots_mut(action)[slot.min(1)] = None;
    }
    /// Restores one action's defaults; a default chord taken meanwhile by
    /// another action is moved back here.
    pub fn reset(&mut self, action: Action) {
        let defaults = default_slots(action);
        for (slot, chord) in defaults.iter().enumerate() {
            match chord {
                Some(chord) => {
                    let _ = self.assign(action, slot, *chord, true);
                }
                None => self.clear(action, slot),
            }
        }
    }
    pub fn reset_all(&mut self) {
        *self = Self::default();
    }
    /// Command for a pressed chord (exact modifiers).
    pub fn command(&self, chord: Chord) -> Option<Action> {
        self.owner(chord).map(|c| c.action).filter(|a| !a.is_drag())
    }
    /// Drag for a pressed mouse button: exact modifiers first, then the
    /// plain button, so Shift+drag still orbits unless bound otherwise.
    pub fn drag(&self, button: MouseButton, mods: Mods) -> Option<Action> {
        [
            Chord {
                input: Input::Mouse(button),
                mods,
            },
            Chord::mouse(button),
        ]
        .into_iter()
        .find_map(|chord| self.owner(chord).map(|c| c.action))
        .filter(|a| a.is_drag())
    }
    /// First chord name of an action, for help texts ("—" when unbound).
    pub fn describe(&self, action: Action) -> String {
        let names: Vec<_> = self
            .slots(action)
            .iter()
            .flatten()
            .map(|c| c.name())
            .collect();
        if names.is_empty() {
            "—".into()
        } else {
            names.join(" / ")
        }
    }

    pub fn to_json(&self) -> Value {
        let mut actions = serde_json::Map::new();
        for (action, slots) in &self.slots {
            actions.insert(
                action.key().into(),
                json!(slots.iter().flatten().map(|c| c.name()).collect::<Vec<_>>()),
            );
        }
        json!({"version": 3, "invert_zoom": self.invert_zoom, "actions": actions})
    }
    /// Reads a bindings file. Unknown actions and unreadable chords are
    /// skipped with a warning (that action keeps its defaults); a conflict
    /// or a wrong version falls back to all defaults.
    pub fn from_json(value: &Value) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let version = value.get("version").and_then(Value::as_u64);
        if !matches!(version, Some(1..=3)) {
            return (
                Self::default(),
                vec!["неизвестная версия файла управления; взяты значения по умолчанию".into()],
            );
        }
        let mut result = Self {
            invert_zoom: value
                .get("invert_zoom")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            ..Self::default()
        };
        let Some(actions) = value.get("actions").and_then(Value::as_object) else {
            return (result, vec!["в файле управления нет действий".into()]);
        };
        let mut parsed = Vec::new();
        let mut middle_moved = false;
        let mut camera_moved = false;
        for (name, chords) in actions {
            let Some(action) = Action::ALL.iter().copied().find(|a| a.key() == name) else {
                warnings.push(format!("неизвестное действие «{name}» пропущено"));
                continue;
            };
            let list: Vec<_> = chords
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let mut chords: Vec<_> = list.iter().map(|t| Chord::parse(t)).collect();
            // Version 1 → 2: plain left/right camera drags move to Shift.
            if version == Some(1) && action.is_drag() {
                for chord in chords.iter_mut().flatten() {
                    if chord.mods == Mods::NONE
                        && matches!(
                            chord.input,
                            Input::Mouse(MouseButton::Left | MouseButton::Right)
                        )
                    {
                        chord.mods.shift = true;
                        result.migrated = true;
                        camera_moved = true;
                    }
                }
            }
            // Version ≤ 2 → 3: the middle button moves from «Вписать» to
            // «Захват» (F still frames).
            if version.is_some_and(|v| v < 3) && action == Action::Fit {
                let middle = Some(Chord::mouse(MouseButton::Middle));
                let before = chords.len();
                chords.retain(|chord| *chord != middle);
                if chords.len() != before {
                    result.migrated = true;
                    middle_moved = true;
                }
            }
            if chords.iter().any(Option::is_none) || chords.len() > 2 {
                warnings.push(format!(
                    "«{}»: непонятная клавиша, оставлено по умолчанию",
                    action.label()
                ));
                continue;
            }
            let mut slots = [None, None];
            for (slot, chord) in chords.into_iter().enumerate() {
                slots[slot] = chord;
            }
            parsed.push((action, slots));
        }
        for (action, _) in &parsed {
            *result.slots_mut(*action) = [None, None];
        }
        let listed: Vec<Action> = parsed.iter().map(|(a, _)| *a).collect();
        for (action, slots) in parsed {
            for (slot, chord) in slots.iter().enumerate() {
                // A new default (not in the file) gives way to the owner's chord.
                if let Some(chord) = chord
                    && let Err(conflict) = result.assign(action, slot, *chord, false)
                    && (listed.contains(&conflict.action)
                        || result.assign(action, slot, *chord, true).is_err())
                {
                    return (
                        Self::default(),
                        vec![format!(
                            "«{}» назначено двум действиям; взяты значения по умолчанию",
                            chord.name()
                        )],
                    );
                }
            }
        }
        if camera_moved {
            warnings.push(
                "ЛКМ и ПКМ теперь — выбор объектов и контекстное меню: вращение и сдвиг с них перенесены на Shift+ЛКМ и Shift+ПКМ".into(),
            );
        }
        if middle_moved {
            warnings.push(
                "СКМ теперь — захват объектов (щелчок — взять, щелчок — положить); «Вписать» — F"
                    .into(),
            );
        }
        (result, warnings)
    }
}

/// Outcome of one press while a slot is being captured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Captured {
    /// A modifier alone: keep waiting for the main key.
    Wait,
    /// Plain Esc: capture ends, nothing changes.
    Cancelled,
    Assigned,
    /// The chord belongs to another action; ask before moving it.
    Conflict(Conflict),
}
pub fn capture(
    bindings: &mut Bindings,
    action: Action,
    slot: usize,
    chord: Chord,
    escape: bool,
    modifier: bool,
) -> Captured {
    if modifier {
        Captured::Wait
    } else if escape && chord.mods == Mods::NONE {
        Captured::Cancelled
    } else {
        match bindings.assign(action, slot, chord, false) {
            Ok(()) => Captured::Assigned,
            Err(owner) => Captured::Conflict(owner),
        }
    }
}

/// %LOCALAPPDATA%\AssetDoctorRTPT\input-bindings.json.
pub fn default_path() -> Option<PathBuf> {
    crate::history::History::default_path().map(|p| p.with_file_name("input-bindings.json"))
}
/// Missing file: defaults, no warning. Unreadable file: defaults + warning.
pub fn load(path: &Path) -> (Bindings, Vec<String>) {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Bindings::default(), Vec::new()),
        Err(e) => (
            Bindings::default(),
            vec![format!("файл управления не прочитан: {e}")],
        ),
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => Bindings::from_json(&value),
            Err(_) => (
                Bindings::default(),
                vec!["файл управления повреждён; взяты значения по умолчанию".into()],
            ),
        },
    }
}
/// Writes a temporary file next to the target, then replaces it.
pub fn save(path: &Path, bindings: &Bindings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_vec_pretty(&bindings.to_json()).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_keep_the_previous_keys_and_have_no_conflicts() {
        let b = Bindings::default();
        let ctrl = |k| Chord::ctrl(k);
        assert_eq!(b.command(ctrl(KeyCode::KeyO)), Some(Action::Open));
        assert_eq!(b.command(ctrl(KeyCode::KeyS)), Some(Action::Save));
        assert_eq!(b.command(ctrl(KeyCode::KeyZ)), Some(Action::Undo));
        assert_eq!(b.command(ctrl(KeyCode::KeyY)), Some(Action::Redo));
        let shift_ctrl_z = Chord::key(
            KeyCode::KeyZ,
            Mods {
                ctrl: true,
                shift: true,
                alt: false,
            },
        );
        assert_eq!(b.command(shift_ctrl_z), Some(Action::Redo));
        assert_eq!(
            b.command(Chord::plain(KeyCode::KeyG)),
            Some(Action::ToggleFloor)
        );
        assert_eq!(
            b.command(Chord::plain(KeyCode::Escape)),
            Some(Action::Cancel)
        );
        // 2026-11-21: side buttons turn and move the camera, Shift + left /
        // right are the fallback; plain left and right stay free (select,
        // context menu); the middle click frames the model.
        assert_eq!(
            b.drag(MouseButton::Forward, Mods::NONE),
            Some(Action::Orbit)
        );
        assert_eq!(b.drag(MouseButton::Back, Mods::NONE), Some(Action::Pan));
        assert_eq!(b.drag(MouseButton::Left, Mods::NONE), None);
        assert_eq!(b.drag(MouseButton::Right, Mods::NONE), None);
        let shift = Mods {
            shift: true,
            ..Mods::NONE
        };
        assert_eq!(b.drag(MouseButton::Left, shift), Some(Action::Orbit));
        assert_eq!(b.drag(MouseButton::Right, shift), Some(Action::Pan));
        // 2026-11-25: the middle button grabs, F frames.
        assert_eq!(
            b.command(Chord::mouse(MouseButton::Middle)),
            Some(Action::Grab)
        );
        assert_eq!(b.command(Chord::plain(KeyCode::KeyF)), Some(Action::Fit));
        assert_eq!(
            b.command(Chord::plain(KeyCode::Tab)),
            Some(Action::ToggleOutline)
        );
        assert_eq!(
            b.command(Chord::plain(KeyCode::KeyX)),
            Some(Action::DeleteObject)
        );
        assert_eq!(
            b.command(Chord::mouse(MouseButton::Left)),
            None,
            "drags are not commands"
        );
        let mut chords = std::collections::HashSet::new();
        for action in Action::ALL {
            for chord in b.slots(action).into_iter().flatten() {
                assert!(chords.insert(chord), "{} twice", chord.name());
            }
        }
    }
    #[test]
    fn conflicts_are_refused_or_moved_and_reset_restores() {
        let mut b = Bindings::default();
        let g = Chord::plain(KeyCode::KeyG);
        assert_eq!(
            b.assign(Action::ToggleWireframe, 0, g, false),
            Err(Conflict {
                action: Action::ToggleFloor,
                slot: 0
            })
        );
        assert_eq!(
            b.command(g),
            Some(Action::ToggleFloor),
            "refusal changes nothing"
        );
        b.assign(Action::ToggleWireframe, 0, g, true).unwrap();
        assert_eq!(b.command(g), Some(Action::ToggleWireframe));
        assert_eq!(b.slots(Action::ToggleFloor), [None, None]);
        b.reset(Action::ToggleFloor);
        assert_eq!(b.command(g), Some(Action::ToggleFloor));
        assert_eq!(b.slots(Action::ToggleWireframe), [None, None]);
        // Side buttons can drive commands and drags.
        b.assign(Action::Undo, 1, Chord::mouse(MouseButton::Back), true)
            .unwrap();
        assert_eq!(
            b.command(Chord::mouse(MouseButton::Back)),
            Some(Action::Undo)
        );
        assert_eq!(
            b.drag(MouseButton::Forward, Mods::NONE),
            Some(Action::Orbit)
        );
        assert_eq!(b.drag(MouseButton::Back, Mods::NONE), None);
        // Re-assigning the same slot is not a conflict.
        b.assign(Action::Undo, 1, Chord::mouse(MouseButton::Back), false)
            .unwrap();
        b.reset_all();
        assert_eq!(b, Bindings::default());
    }
    #[test]
    fn version_one_files_move_plain_left_and_right_camera_drags_to_shift() {
        // The owner's file of 2026-11-20: left/right and the side buttons.
        let v1 = json!({"version": 1, "invert_zoom": false, "actions": {
            "orbit": ["Mouse Left", "Mouse Forward"],
            "pan": ["Mouse Right", "Mouse Back"],
            "fit": ["F", "Mouse Middle"],
            "toggle_wireframe": ["X"]
        }});
        let (b, warnings) = Bindings::from_json(&v1);
        assert!(b.migrated);
        // Camera drags moved to Shift, and the middle button to «Захват».
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(
            b.command(Chord::mouse(MouseButton::Middle)),
            Some(Action::Grab)
        );
        assert_eq!(
            b.slots(Action::Fit),
            [Some(Chord::plain(KeyCode::KeyF)), None]
        );
        let shift = Mods {
            shift: true,
            ..Mods::NONE
        };
        assert_eq!(b.drag(MouseButton::Left, Mods::NONE), None);
        assert_eq!(b.drag(MouseButton::Left, shift), Some(Action::Orbit));
        assert_eq!(b.drag(MouseButton::Right, shift), Some(Action::Pan));
        assert_eq!(
            b.drag(MouseButton::Forward, Mods::NONE),
            Some(Action::Orbit)
        );
        // The owner's X keeps its meaning; the new delete default gives way.
        assert_eq!(
            b.command(Chord::plain(KeyCode::KeyX)),
            Some(Action::ToggleWireframe)
        );
        assert_eq!(
            b.command(Chord::plain(KeyCode::Tab)),
            Some(Action::ToggleOutline)
        );
        // Saved again as version 3 and read back unchanged.
        let (again, warnings) = Bindings::from_json(&b.to_json());
        assert!(warnings.is_empty() && !again.migrated);
        assert_eq!(again.slots(Action::Orbit), b.slots(Action::Orbit));
    }
    #[test]
    fn version_two_files_give_the_middle_button_to_grab_and_z_to_the_skeleton() {
        let v2 = json!({"version": 2, "invert_zoom": false, "actions": {
            "fit": ["F", "Mouse Middle"],
            "orbit": ["Mouse Forward", "Shift+Mouse Left"]
        }});
        let (b, warnings) = Bindings::from_json(&v2);
        assert!(b.migrated);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("СКМ"));
        assert_eq!(
            b.command(Chord::mouse(MouseButton::Middle)),
            Some(Action::Grab)
        );
        assert_eq!(b.command(Chord::plain(KeyCode::KeyF)), Some(Action::Fit));
        assert_eq!(
            b.command(Chord::plain(KeyCode::KeyZ)),
            Some(Action::ToggleSkeleton)
        );
        assert_eq!(
            b.command(Chord::plain(KeyCode::KeyQ)),
            Some(Action::ToggleLeftPanel)
        );
        assert_eq!(
            b.command(Chord::plain(KeyCode::KeyE)),
            Some(Action::ToggleRightPanel)
        );
        // Undo keeps Ctrl+Z; plain Z is the skeleton.
        assert_eq!(b.command(Chord::ctrl(KeyCode::KeyZ)), Some(Action::Undo));
        // A version 3 file keeps whatever the owner put on the middle button.
        let mut v3 = b.to_json();
        v3["actions"]["fit"] = json!(["F", "Mouse Middle"]);
        v3["actions"]["grab"] = json!(["G"]);
        v3["actions"]["toggle_floor"] = json!([]);
        let (kept, warnings) = Bindings::from_json(&v3);
        assert!(warnings.is_empty() && !kept.migrated, "{warnings:?}");
        assert_eq!(
            kept.command(Chord::mouse(MouseButton::Middle)),
            Some(Action::Fit)
        );
    }
    #[test]
    fn capture_waits_for_modifiers_cancels_on_esc_and_reports_conflicts() {
        let mut b = Bindings::default();
        let a = Action::ToggleAxes;
        let ctrl = Mods {
            ctrl: true,
            ..Mods::NONE
        };
        let key = |k| Chord::key(k, Mods::NONE);
        assert_eq!(
            capture(&mut b, a, 0, key(KeyCode::ControlLeft), false, true),
            Captured::Wait
        );
        assert_eq!(
            capture(&mut b, a, 0, key(KeyCode::Escape), true, false),
            Captured::Cancelled
        );
        assert_eq!(b, Bindings::default());
        let shift_esc = Chord::key(
            KeyCode::Escape,
            Mods {
                shift: true,
                ..Mods::NONE
            },
        );
        assert_eq!(
            capture(&mut b, a, 0, shift_esc, true, false),
            Captured::Assigned
        );
        assert_eq!(
            capture(&mut b, a, 1, Chord::key(KeyCode::KeyS, ctrl), false, false),
            Captured::Conflict(Conflict {
                action: Action::Save,
                slot: 0
            })
        );
        assert_eq!(b.slots(a), [Some(shift_esc), None]);
    }
    #[test]
    fn names_round_trip_and_files_load_safely() {
        for action in Action::ALL {
            for chord in default_slots(action).into_iter().flatten() {
                assert_eq!(Chord::parse(&chord.name()), Some(chord));
            }
        }
        for (key, _) in KEYS {
            let chord = Chord::plain(*key);
            assert_eq!(Chord::parse(&chord.name()), Some(chord));
        }
        assert_eq!(
            Chord::parse("Alt+Mouse 7"),
            Some(Chord {
                input: Input::Mouse(MouseButton::Other(7)),
                mods: Mods {
                    alt: true,
                    ..Mods::NONE
                }
            })
        );
        assert_eq!(Chord::parse("Ctrl+Hyper"), None);

        let mut b = Bindings {
            invert_zoom: true,
            ..Bindings::default()
        };
        b.assign(Action::ToggleAxes, 0, Chord::plain(KeyCode::KeyJ), false)
            .unwrap();
        let (back, warnings) = Bindings::from_json(&b.to_json());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back, b);

        let mut v = b.to_json();
        v["actions"]["fit"] = json!(["Ctrl+Hyper"]);
        v["actions"]["teleport"] = json!(["T"]);
        let (back, warnings) = Bindings::from_json(&v);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(back.slots(Action::Fit), default_slots(Action::Fit));
        let mut v = b.to_json();
        v["actions"]["fit"] = json!(["G"]);
        let (back, warnings) = Bindings::from_json(&v);
        assert_eq!(back, Bindings::default(), "conflict falls back to defaults");
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            Bindings::from_json(&json!({"version": 9})).0,
            Bindings::default()
        );

        let dir = std::env::temp_dir().join(format!("ad-input-{}", std::process::id()));
        let path = dir.join("input-bindings.json");
        assert_eq!(
            load(&path),
            (Bindings::default(), Vec::new()),
            "missing file"
        );
        save(&path, &b).unwrap();
        assert_eq!(load(&path).0, b);
        std::fs::write(&path, b"{not json").unwrap();
        let (back, warnings) = load(&path);
        assert_eq!(back, Bindings::default());
        assert_eq!(warnings.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
