//! View settings kept between runs (P6): shading, channel and maps, grid and
//! helpers, lamp, exposure, shadows, environment light, floor, sky and the
//! owner's skybox file, the model gizmo switch and the layout (right block,
//! tab, health page, tree). Stored in
//! %LOCALAPPDATA%\AssetDoctorRTPT\view-settings.json. View only: nothing here
//! touches the model, its placement or the camera.
use super::{HealthPage, Tab, ViewerUi};
use ad_render_vulkan::lighting::{Channel, ShadingMode};
use ad_render_vulkan::{grid::FloorTheme, sky::Environment};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const VERSION: u64 = 1;

/// %LOCALAPPDATA%\AssetDoctorRTPT\view-settings.json.
pub fn default_path() -> Option<PathBuf> {
    crate::history::History::default_path().map(|p| p.with_file_name("view-settings.json"))
}
/// Missing file: None, no warning. Unreadable or damaged: None + warning.
pub fn load(path: &Path) -> (Option<Value>, Vec<String>) {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (None, Vec::new()),
        Err(e) => (None, vec![format!("файл настроек вида не прочитан: {e}")]),
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => (Some(value), Vec::new()),
            Err(_) => (
                None,
                vec!["файл настроек вида повреждён; взяты значения по умолчанию".into()],
            ),
        },
    }
}
/// Writes a temporary file next to the target, then replaces it.
pub fn save(path: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

fn index_of<T: PartialEq + Copy>(all: &[T], value: T) -> usize {
    all.iter().position(|v| *v == value).unwrap_or(0)
}

impl ViewerUi {
    /// The current view settings as stored JSON.
    pub fn view_settings(&self) -> Value {
        let l = &self.lighting;
        json!({
            "version": VERSION,
            "shading": if l.mode == ShadingMode::DirectPbr { "pbr" } else { "base" },
            "channel": index_of(&Channel::ALL, self.channel),
            "textures": self.textures,
            "mr_maps": self.mr_maps,
            "normal_maps": self.normal_maps,
            "ao_maps": self.ao_maps,
            "emission": self.emission,
            "wireframe": self.wireframe,
            "axes": self.show_axes,
            "bounds": self.show_bounds,
            "skeleton": self.skeleton_on,
            "joint_axes_local": self.joint_local_axes,
            "joint_gizmo_size": self.joint_gizmo_size,
            "outline_width": self.outline_width,
            "ui_scale": self.ui_scale,
            "autoplay": self.autoplay,
            "root_path": self.root_path.show,
            "window": self.window_mode.key(),
            "frameless": self.frameless,
            "lamp": {
                "enabled": l.enabled,
                "direction": l.direction,
                "color": l.color,
                "kelvin": self.lamp_kelvin,
                "distance": l.distance,
                "icon": self.lamp_icon,
            },
            "exposure_ev": l.exposure_ev,
            "key": l.key,
            "fill": l.fill,
            "shadows": self.shadows,
            "shadow_strength": self.shadow_strength,
            "environment": index_of(&Environment::ALL, self.environment),
            "environment_light": self.environment_light,
            "environment_strength": self.environment_strength,
            "floor": self.floor.map(|f| index_of(&FloorTheme::ALL, f)),
            "last_floor": index_of(&FloorTheme::ALL, self.last_floor),
            "skybox_width": self.skybox_width,
            "skybox_folder": self.skybox_folder.as_ref().map(|p| p.to_string_lossy()),
            "skybox": self.skybox_path.as_ref().map(|p| json!({
                "path": p.to_string_lossy(),
                "on": self.skybox_on,
                "yaw": self.skybox_yaw,
            })),
            "model_gizmo": self.model_gizmo_on,
            "log_to_file": self.log_to_file,
            "check_updates": self.update.check_on_start,
            "outline": self.outline_on,
            "outline_color": self.outline_color,
            "layout": {
                "right_block": self.show_summary,
                "tab": match self.tab {
                    Tab::Health => "health",
                    Tab::Info => "info",
                    Tab::Rigging => "rigging",
                },
                "health_page": if self.health_page == HealthPage::Repair { "repair" } else { "analysis" },
                "tree": self.show_tree,
            },
        })
    }

    /// Applies stored settings field by field: a missing or invalid field
    /// keeps its current value and is reported. Returns the skybox file to
    /// load again (with its «on» flag), if any.
    pub fn apply_view_settings(
        &mut self,
        value: &Value,
        warnings: &mut Vec<String>,
    ) -> Option<(PathBuf, bool)> {
        let Some(object) = value.as_object() else {
            warnings.push(
                "файл настроек вида не является объектом; взяты значения по умолчанию".into(),
            );
            return None;
        };
        if object.get("version").and_then(Value::as_u64) != Some(VERSION) {
            warnings
                .push("неизвестная версия файла настроек вида; взяты значения по умолчанию".into());
            return None;
        }
        let mut bad = Vec::new();
        let flag = |v: &Value, key: &str, target: &mut bool, bad: &mut Vec<String>| match v.get(key)
        {
            None => {}
            Some(Value::Bool(b)) => *target = *b,
            Some(_) => bad.push(key.to_string()),
        };
        let number = |v: &Value,
                      key: &str,
                      range: std::ops::RangeInclusive<f32>,
                      target: &mut f32,
                      bad: &mut Vec<String>| {
            match v.get(key) {
                None => {}
                Some(n) => match n.as_f64().map(|n| n as f32) {
                    Some(n) if n.is_finite() && range.contains(&n) => *target = n,
                    _ => bad.push(key.to_string()),
                },
            }
        };
        let choice = |v: &Value, key: &str, len: usize, bad: &mut Vec<String>| -> Option<usize> {
            match v.get(key) {
                None => None,
                Some(n) => match n.as_u64().map(|n| n as usize) {
                    Some(i) if i < len => Some(i),
                    _ => {
                        bad.push(key.to_string());
                        None
                    }
                },
            }
        };
        match value.get("shading").and_then(Value::as_str) {
            Some("pbr") => self.lighting.mode = ShadingMode::DirectPbr,
            Some("base") => self.lighting.mode = ShadingMode::BaseColor,
            None if value.get("shading").is_none() => {}
            _ => bad.push("shading".into()),
        }
        if let Some(i) = choice(value, "channel", Channel::ALL.len(), &mut bad) {
            self.channel = Channel::ALL[i];
        }
        flag(value, "textures", &mut self.textures, &mut bad);
        flag(value, "mr_maps", &mut self.mr_maps, &mut bad);
        flag(value, "normal_maps", &mut self.normal_maps, &mut bad);
        flag(value, "ao_maps", &mut self.ao_maps, &mut bad);
        flag(value, "emission", &mut self.emission, &mut bad);
        flag(value, "wireframe", &mut self.wireframe, &mut bad);
        flag(value, "axes", &mut self.show_axes, &mut bad);
        flag(value, "bounds", &mut self.show_bounds, &mut bad);
        flag(value, "skeleton", &mut self.skeleton_on, &mut bad);
        flag(
            value,
            "joint_axes_local",
            &mut self.joint_local_axes,
            &mut bad,
        );
        if value.get("outline_width").is_some() {
            number(
                value,
                "outline_width",
                ad_render_vulkan::outline::WIDTHS,
                &mut self.outline_width,
                &mut bad,
            );
        }
        number(value, "ui_scale", 0.75..=2.0, &mut self.ui_scale, &mut bad);
        flag(value, "autoplay", &mut self.autoplay, &mut bad);
        flag(value, "root_path", &mut self.root_path.show, &mut bad);
        flag(value, "frameless", &mut self.frameless, &mut bad);
        number(
            value,
            "joint_gizmo_size",
            0.5..=2.0,
            &mut self.joint_gizmo_size,
            &mut bad,
        );
        match value.get("window") {
            None => {}
            Some(Value::String(key)) if super::WindowMode::from_key(key).is_some() => {
                self.window_mode = super::WindowMode::from_key(key).unwrap_or_default();
            }
            Some(_) => bad.push("window".into()),
        }
        let mut lighting = self.lighting;
        number(
            value,
            "exposure_ev",
            -6.0..=6.0,
            &mut lighting.exposure_ev,
            &mut bad,
        );
        number(value, "key", 0.0..=8.0, &mut lighting.key, &mut bad);
        number(value, "fill", 0.0..=1.0, &mut lighting.fill, &mut bad);
        if let Some(lamp) = value.get("lamp") {
            flag(lamp, "enabled", &mut lighting.enabled, &mut bad);
            flag(lamp, "icon", &mut self.lamp_icon, &mut bad);
            number(
                lamp,
                "kelvin",
                1500.0..=10000.0,
                &mut self.lamp_kelvin,
                &mut bad,
            );
            number(
                lamp,
                "distance",
                ad_render_vulkan::lighting::LAMP_DISTANCES,
                &mut lighting.distance,
                &mut bad,
            );
            let vector = |key: &str| -> Option<[f32; 3]> {
                let a = lamp.get(key)?.as_array()?;
                (a.len() == 3).then_some(())?;
                let v: Vec<f32> = a
                    .iter()
                    .filter_map(|n| n.as_f64().map(|n| n as f32))
                    .collect();
                (v.len() == 3).then(|| [v[0], v[1], v[2]])
            };
            if lamp.get("direction").is_some() {
                match vector("direction").and_then(|d| {
                    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    (length.is_finite() && length > 0.5).then(|| {
                        // A saved unit vector is kept bit-exact.
                        if (length - 1.).abs() < 1e-4 {
                            d
                        } else {
                            d.map(|v| v / length)
                        }
                    })
                }) {
                    Some(d) => lighting.direction = d,
                    None => bad.push("lamp.direction".into()),
                }
            }
            if lamp.get("color").is_some() {
                match vector("color").filter(|c| c.iter().all(|v| (0.0..=1.0).contains(v))) {
                    Some(c) => lighting.color = c,
                    None => bad.push("lamp.color".into()),
                }
            }
        }
        if lighting.valid() {
            self.lighting = lighting;
        } else {
            bad.push("lighting".into());
        }
        flag(value, "shadows", &mut self.shadows, &mut bad);
        number(
            value,
            "shadow_strength",
            0.0..=1.0,
            &mut self.shadow_strength,
            &mut bad,
        );
        if let Some(i) = choice(value, "environment", Environment::ALL.len(), &mut bad) {
            self.environment = Environment::ALL[i];
        }
        flag(
            value,
            "environment_light",
            &mut self.environment_light,
            &mut bad,
        );
        number(
            value,
            "environment_strength",
            0.0..=ad_render_vulkan::sky::EnvironmentLight::MAX_STRENGTH,
            &mut self.environment_strength,
            &mut bad,
        );
        match value.get("floor") {
            None => {}
            Some(Value::Null) => self.floor = None,
            Some(n) => match n.as_u64().map(|n| n as usize) {
                Some(i) if i < FloorTheme::ALL.len() => self.floor = Some(FloorTheme::ALL[i]),
                _ => bad.push("floor".into()),
            },
        }
        if let Some(i) = choice(value, "last_floor", FloorTheme::ALL.len(), &mut bad) {
            self.last_floor = FloorTheme::ALL[i];
        }
        match value.get("skybox_width") {
            None => {}
            Some(n) => match n.as_u64().map(|n| n as u32) {
                Some(w) if ad_render_vulkan::ibl::WIDTHS.contains(&w) => self.skybox_width = w,
                _ => bad.push("skybox_width".into()),
            },
        }
        match value.get("skybox_folder") {
            None | Some(Value::Null) => {}
            Some(Value::String(path)) if !path.is_empty() => {
                self.skybox_folder = Some(PathBuf::from(path));
            }
            Some(_) => bad.push("skybox_folder".into()),
        }
        let mut skybox = None;
        match value.get("skybox") {
            None | Some(Value::Null) => {}
            Some(s) => match s.get("path").and_then(Value::as_str) {
                Some(path) if !path.is_empty() => {
                    number(s, "yaw", 0.0..=360.0, &mut self.skybox_yaw, &mut bad);
                    let mut on = true;
                    flag(s, "on", &mut on, &mut bad);
                    skybox = Some((PathBuf::from(path), on));
                }
                _ => bad.push("skybox".into()),
            },
        }
        flag(value, "model_gizmo", &mut self.model_gizmo_on, &mut bad);
        flag(value, "log_to_file", &mut self.log_to_file, &mut bad);
        flag(
            value,
            "check_updates",
            &mut self.update.check_on_start,
            &mut bad,
        );
        flag(value, "outline", &mut self.outline_on, &mut bad);
        match value.get("outline_color") {
            None => {}
            Some(Value::Null) => self.outline_color = None,
            Some(n) => match n.as_u64().map(|n| n as usize) {
                Some(i) if i < super::OUTLINE_COLORS.len() => self.outline_color = Some(i),
                _ => bad.push("outline_color".into()),
            },
        }
        if let Some(layout) = value.get("layout") {
            flag(layout, "right_block", &mut self.show_summary, &mut bad);
            flag(layout, "tree", &mut self.show_tree, &mut bad);
            match layout.get("tab").and_then(Value::as_str) {
                Some("info") => self.tab = Tab::Info,
                Some("rigging") => self.tab = Tab::Rigging,
                Some("health") => self.tab = Tab::Health,
                None if layout.get("tab").is_none() => {}
                _ => bad.push("layout.tab".into()),
            }
            match layout.get("health_page").and_then(Value::as_str) {
                Some("repair") => self.health_page = HealthPage::Repair,
                Some("analysis") => self.health_page = HealthPage::Analysis,
                None if layout.get("health_page").is_none() => {}
                _ => bad.push("layout.health_page".into()),
            }
        }
        if !bad.is_empty() {
            warnings.push(format!(
                "в настройках вида пропущены неверные значения: {}",
                bad.join(", ")
            ));
        }
        skybox
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_round_trip_through_the_file() {
        let mut ui = ViewerUi {
            textures: false,
            wireframe: true,
            show_axes: true,
            shadows: false,
            shadow_strength: 0.3,
            environment: Environment::ALL[2],
            environment_light: true,
            environment_strength: 1.5,
            floor: Some(FloorTheme::ALL[5]),
            lamp_kelvin: 3200.,
            lamp_icon: false,
            model_gizmo_on: false,
            show_summary: false,
            show_tree: true,
            tab: Tab::Info,
            channel: Channel::ALL[3],
            skybox_path: Some(PathBuf::from("C:/panoramas/sky.png")),
            skybox_on: false,
            skybox_yaw: 90.,
            skybox_width: 8192,
            skybox_folder: Some(PathBuf::from("C:/panoramas")),
            ..Default::default()
        };
        ui.health_page = HealthPage::Repair;
        ui.last_floor = FloorTheme::ALL[7];
        ui.lighting.mode = ShadingMode::DirectPbr;
        ui.lighting.direction = crate::lamp::from_angles(-40., 25.);
        ui.lighting.color = [1., 0.8, 0.6];
        ui.lighting.enabled = false;
        ui.lighting.exposure_ev = 1.5;
        let dir = std::env::temp_dir().join(format!("ad-view-settings-{}", std::process::id()));
        let path = dir.join("view-settings.json");
        save(&path, &ui.view_settings()).unwrap();
        let (value, warnings) = load(&path);
        assert!(warnings.is_empty());
        let mut back = ViewerUi::default();
        let mut warnings = Vec::new();
        let skybox = back.apply_view_settings(&value.unwrap(), &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(skybox, Some((PathBuf::from("C:/panoramas/sky.png"), false)));
        back.skybox_path = skybox.map(|s| s.0);
        back.skybox_on = false;
        assert_eq!(back.view_settings(), ui.view_settings());
        assert_eq!(back.lighting, ui.lighting);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn missing_damaged_or_wrong_values_keep_defaults_with_a_warning() {
        let dir = std::env::temp_dir().join(format!("ad-view-bad-{}", std::process::id()));
        let path = dir.join("view-settings.json");
        assert_eq!(
            load(&path),
            (None, Vec::new()),
            "missing file: silent defaults"
        );
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        let (value, warnings) = load(&path);
        assert!(value.is_none() && warnings.len() == 1);
        std::fs::remove_dir_all(&dir).unwrap();
        // Field by field: good values apply, bad ones are named and skipped.
        let mut ui = ViewerUi::default();
        let defaults = ViewerUi::default();
        let mut warnings = Vec::new();
        ui.apply_view_settings(
            &json!({
                "version": 1,
                "wireframe": true,
                "floor": 999,
                "shadow_strength": 7.0,
                "lamp": {"direction": [0, 0, 0], "color": [2, 0, 0], "enabled": false},
                "layout": {"tab": "nowhere"},
                "unknown": 1
            }),
            &mut warnings,
        );
        assert!(ui.wireframe && !ui.lighting.enabled);
        assert_eq!(ui.floor, defaults.floor);
        assert_eq!(ui.shadow_strength, defaults.shadow_strength);
        assert_eq!(ui.lighting.direction, defaults.lighting.direction);
        assert_eq!(ui.lighting.color, defaults.lighting.color);
        assert_eq!(ui.tab, defaults.tab);
        assert_eq!(warnings.len(), 1);
        for key in [
            "floor",
            "shadow_strength",
            "lamp.direction",
            "lamp.color",
            "layout.tab",
        ] {
            assert!(warnings[0].contains(key), "{key}: {warnings:?}");
        }
        // Another version or shape: nothing applied.
        let mut other = ViewerUi::default();
        let mut warnings = Vec::new();
        other.apply_view_settings(&json!({"version": 2, "wireframe": true}), &mut warnings);
        other.apply_view_settings(&json!([1, 2]), &mut warnings);
        assert!(!other.wireframe && warnings.len() == 2);
        // «Без поверхности» is kept as such.
        let mut none = ViewerUi::default();
        none.apply_view_settings(&json!({"version": 1, "floor": null}), &mut Vec::new());
        assert_eq!(none.floor, None);
    }
}
