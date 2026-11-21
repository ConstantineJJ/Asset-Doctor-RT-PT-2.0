//! Session-only inspection lighting; never source material or scene edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShadingMode {
    #[default]
    BaseColor,
    DirectPbr,
}
/// ADR-0042 render channel: what the viewport shows instead of lit shading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    #[default]
    Lit,
    Normals,
    Uv,
    BaseColor,
    Metallic,
    Roughness,
    Occlusion,
    Emission,
}
impl Channel {
    pub const ALL: [Self; 8] = [
        Self::Lit,
        Self::Normals,
        Self::Uv,
        Self::BaseColor,
        Self::Metallic,
        Self::Roughness,
        Self::Occlusion,
        Self::Emission,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Lit => "Освещение",
            Self::Normals => "Нормали",
            Self::Uv => "UV",
            Self::BaseColor => "Базовый цвет",
            Self::Metallic => "Металличность",
            Self::Roughness => "Шероховатость",
            Self::Occlusion => "Окклюзия · AO",
            Self::Emission => "Свечение",
        }
    }
    /// Index read by mesh.frag from the frame uniform.
    pub(crate) fn index(self) -> f32 {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0) as f32
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewLighting {
    pub mode: ShadingMode,
    pub exposure_ev: f32,
    pub key: f32,
    pub fill: f32,
    /// ADR-0040 lamp: unit world direction towards the lamp.
    pub direction: [f32; 3],
    /// Lamp colour, display RGB 0..=1 (white keeps the previous look).
    pub color: [f32; 3],
    /// Lamp switched on; off leaves only fill and environment light.
    pub enabled: bool,
    /// Point lamp (2026-11-21): distance from the model centre in model
    /// radii; light falls off with the square of the distance and equals the
    /// former sun-like lamp at `DEFAULT_LAMP_DISTANCE`.
    pub distance: f32,
}
/// Lamp distance range and default, in model radii.
pub const LAMP_DISTANCES: std::ops::RangeInclusive<f32> = 1.2..=5.0;
pub const DEFAULT_LAMP_DISTANCE: f32 = 2.5;
/// The lamp direction used before the lamp became a scene object.
pub const DEFAULT_LAMP_DIRECTION: [f32; 3] = [0.423_999_15, 0.847_998_3, 0.317_999_36];
impl Default for ViewLighting {
    fn default() -> Self {
        Self {
            mode: ShadingMode::BaseColor,
            exposure_ev: 0.,
            key: 3.,
            fill: 0.15,
            direction: DEFAULT_LAMP_DIRECTION,
            color: [1.; 3],
            enabled: true,
            distance: DEFAULT_LAMP_DISTANCE,
        }
    }
}
impl ViewLighting {
    pub fn valid(self) -> bool {
        self.exposure_ev.is_finite()
            && (-6.0..=6.0).contains(&self.exposure_ev)
            && self.key.is_finite()
            && (0.0..=8.0).contains(&self.key)
            && self.fill.is_finite()
            && (0.0..=1.0).contains(&self.fill)
            && self.direction.iter().all(|v| v.is_finite())
            && (self.direction.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 1e-3
            && self.color.iter().all(|c| (0.0..=1.0).contains(c))
            && LAMP_DISTANCES.contains(&self.distance)
    }
    pub(crate) fn flags(
        self,
        textures: bool,
        mr_maps: bool,
        normal_maps: bool,
        occlusion_maps: bool,
        emission: bool,
        encode_srgb: bool,
    ) -> f32 {
        // GLSL reads these exact small integers from the float push slot.
        (u32::from(self.mode == ShadingMode::DirectPbr)
            | (u32::from(textures) << 1)
            | (u32::from(encode_srgb) << 2)
            | (u32::from(mr_maps && self.mode == ShadingMode::DirectPbr) << 3)
            | (u32::from(normal_maps && self.mode == ShadingMode::DirectPbr) << 4)
            | (u32::from(occlusion_maps && self.mode == ShadingMode::DirectPbr) << 5)
            | (u32::from(emission && self.mode == ShadingMode::DirectPbr) << 6)) as f32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channels_have_unique_labels_and_stable_indices() {
        let labels: std::collections::BTreeSet<_> =
            Channel::ALL.iter().map(|c| c.label()).collect();
        assert_eq!(labels.len(), Channel::ALL.len());
        for (i, c) in Channel::ALL.iter().enumerate() {
            assert_eq!(c.index(), i as f32);
        }
        assert_eq!(Channel::default(), Channel::Lit);
    }
    #[test]
    fn settings_refuse_nonfinite_and_out_of_range_values_and_pack_independent_flags() {
        let mut view = ViewLighting::default();
        assert!(view.valid());
        assert_eq!(view.flags(true, true, true, true, true, true), 6.);
        view.mode = ShadingMode::DirectPbr;
        assert_eq!(view.flags(false, false, false, false, false, true), 5.);
        assert_eq!(view.flags(false, true, false, false, false, true), 13.);
        assert_eq!(view.flags(false, true, true, false, false, true), 29.);
        assert_eq!(view.flags(false, true, true, true, false, true), 61.);
        assert_eq!(view.flags(false, true, true, true, true, true), 125.);
        assert_eq!(view.flags(true, true, true, true, true, true), 127.);
        for v in [f32::NAN, f32::INFINITY, -6.1, 6.1] {
            view.exposure_ev = v;
            assert!(!view.valid());
        }
        view.exposure_ev = 0.;
        view.key = -1.;
        assert!(!view.valid());
        view.key = 3.;
        view.fill = 1.1;
        assert!(!view.valid());
        view.fill = 0.15;
        assert!(view.valid());
        let d = DEFAULT_LAMP_DIRECTION;
        let legacy = [0.4f32, 0.8, 0.3];
        let len = legacy.iter().map(|v| v * v).sum::<f32>().sqrt();
        for i in 0..3 {
            assert!(
                (d[i] - legacy[i] / len).abs() < 1e-6,
                "default lamp unchanged"
            );
        }
        for direction in [[0., 0., 0.], [0., 2., 0.], [f32::NAN, 1., 0.]] {
            view.direction = direction;
            assert!(!view.valid());
        }
        view.direction = [0., 1., 0.];
        assert!(view.valid());
        view.color = [1.2, 1., 1.];
        assert!(!view.valid());
    }
}
