//! Session-only inspection lighting; never source material or scene edits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShadingMode {
    #[default]
    BaseColor,
    DirectPbr,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewLighting {
    pub mode: ShadingMode,
    pub exposure_ev: f32,
    pub key: f32,
    pub fill: f32,
}
impl Default for ViewLighting {
    fn default() -> Self {
        Self {
            mode: ShadingMode::BaseColor,
            exposure_ev: 0.,
            key: 3.,
            fill: 0.15,
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
    }
}
