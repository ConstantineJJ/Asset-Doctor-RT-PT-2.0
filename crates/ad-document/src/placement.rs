//! ADR-0041 model placement: one uniform scale and translation above every
//! scene root. Primitive data, the hierarchy and non-root nodes never change.
use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode, MeshId,
    PreparedRemoval, RemovalPreview, indices::Alternate,
};

/// `x' = scale * x + translation`, applied above the scene roots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub scale: f32,
    pub translation: [f32; 3],
}
impl Default for Placement {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Placement {
    pub const IDENTITY: Self = Self {
        scale: 1.,
        translation: [0.; 3],
    };
    pub const MIN_SCALE: f32 = 1e-6;
    pub const MAX_SCALE: f32 = 1e6;
    pub const MAX_TRANSLATION: f32 = 1e12;
    pub fn is_identity(self) -> bool {
        self == Self::IDENTITY
    }
    pub fn valid(self) -> bool {
        self.scale.is_finite()
            && (Self::MIN_SCALE..=Self::MAX_SCALE).contains(&self.scale)
            && self
                .translation
                .iter()
                .all(|v| v.is_finite() && v.abs() <= Self::MAX_TRANSLATION)
    }
    pub fn matrix(self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::splat(self.scale),
            glam::Quat::IDENTITY,
            glam::Vec3::from_array(self.translation),
        )
    }
    pub fn apply(self, point: [f32; 3]) -> [f32; 3] {
        let [x, y, z] = point;
        let [tx, ty, tz] = self.translation;
        [
            self.scale * x + tx,
            self.scale * y + ty,
            self.scale * z + tz,
        ]
    }
    /// `P * matrix` for a column-major local transform.
    pub fn apply_matrix(self, matrix: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
        (self.matrix() * glam::Mat4::from_cols_array_2d(&matrix)).to_cols_array_2d()
    }
    /// The inverse mapping (for view helpers such as the floor height).
    pub fn unapply(self, point: [f32; 3]) -> [f32; 3] {
        let [x, y, z] = point;
        let [tx, ty, tz] = self.translation;
        [
            (x - tx) / self.scale,
            (y - ty) / self.scale,
            (z - tz) / self.scale,
        ]
    }
    /// This placement followed by scaling `factor` about `pivot` and moving
    /// by `offset` (world units): `x'' = factor * (x' - pivot) + pivot + offset`.
    pub fn then(self, pivot: [f32; 3], factor: f32, offset: [f32; 3]) -> Self {
        let shift = |axis: usize| {
            factor * self.translation[axis] + pivot[axis] - factor * pivot[axis] + offset[axis]
        };
        Self {
            scale: self.scale * factor,
            translation: [shift(0), shift(1), shift(2)],
        }
    }
}

impl AssetDocument {
    /// Current placement of the model (identity after import).
    pub fn placement(&self) -> Placement {
        self.placement
    }
    /// ADR-0041: proposal to replace the placement. Same placement is a
    /// verified no-op; invalid values and documents without primitives are
    /// refused. History keeps only the previous placement.
    pub fn prepare_placement(
        &self,
        expected: DocumentTag,
        placement: Placement,
        cancel: &Cancellation,
    ) -> AssetResult<PreparedRemoval> {
        self.verify_tag(expected, cancel)?;
        if !placement.valid() {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "Placement must be finite with a scale between 1e-6 and 1e6",
            ));
        }
        let first = self.parts.meshes.first().ok_or_else(|| {
            AssetError::new(
                ErrorCode::Unsupported,
                "A model without primitives cannot be placed",
            )
        })?;
        let triangles = first.indices.len() / 3;
        let replacement = (placement != self.placement).then_some(Alternate::Placement(placement));
        Ok(PreparedRemoval::new(
            RemovalPreview {
                tag: expected,
                mesh: MeshId(0),
                before_triangles: triangles,
                after_triangles: triangles,
                before_vertices: first.positions.len(),
                after_vertices: first.positions.len(),
            },
            self.origin.clone(),
            replacement,
            std::mem::size_of::<Placement>(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_composes_about_a_pivot_and_validates_limits() {
        let p = Placement::IDENTITY.then([1., 0., 1.], 2., [0., 3., 0.]);
        // The pivot stays where it was, then moves by the offset.
        assert_eq!(p.apply([1., 0., 1.]), [1., 3., 1.]);
        assert_eq!(p.apply([2., 0., 1.]), [3., 3., 1.]);
        let q = p.then([0.; 3], 0.5, [0.; 3]);
        assert_eq!(q.scale, 1.);
        let back = q.unapply(q.apply([4., -2., 7.]));
        assert!(
            back.iter()
                .zip([4., -2., 7.])
                .all(|(a, b)| (a - b).abs() < 1e-5)
        );
        let m = p.matrix().transform_point3(glam::Vec3::new(2., 0., 1.));
        assert_eq!(m.to_array(), [3., 3., 1.]);
        assert!(Placement::IDENTITY.valid() && Placement::IDENTITY.is_identity());
        for bad in [
            Placement {
                scale: 0.,
                ..Placement::IDENTITY
            },
            Placement {
                scale: f32::NAN,
                ..Placement::IDENTITY
            },
            Placement {
                scale: 2e6,
                ..Placement::IDENTITY
            },
            Placement {
                scale: 1.,
                translation: [f32::INFINITY, 0., 0.],
            },
            Placement {
                scale: 1.,
                translation: [0., 2e12, 0.],
            },
        ] {
            assert!(!bad.valid(), "{bad:?}");
        }
    }
}
