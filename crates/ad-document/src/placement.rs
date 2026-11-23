//! ADR-0041 model placement: one uniform scale, rotation and translation
//! above every scene root. Primitive data, the hierarchy and non-root nodes
//! never change.
use crate::{
    AssetDocument, AssetError, AssetResult, Cancellation, DocumentTag, ErrorCode, MeshId,
    PreparedRemoval, RemovalPreview, indices::Alternate,
};

/// `x' = scale * rotation(x) + translation`, applied above the scene roots.
/// `rotation` is a unit quaternion `[x, y, z, w]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub scale: f32,
    pub rotation: [f32; 4],
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
        rotation: [0., 0., 0., 1.],
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
            && self.rotation.iter().all(|v| v.is_finite())
            && (glam::Quat::from_array(self.rotation).length_squared() - 1.).abs() < 1e-3
    }
    pub fn quat(self) -> glam::Quat {
        glam::Quat::from_array(self.rotation)
    }
    pub fn matrix(self) -> glam::Mat4 {
        glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::splat(self.scale),
            self.quat(),
            glam::Vec3::from_array(self.translation),
        )
    }
    pub fn apply(self, point: [f32; 3]) -> [f32; 3] {
        let [x, y, z] = (self.quat() * glam::Vec3::from_array(point)).to_array();
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
    /// This rotation followed by a node's own unit rotation `[x, y, z, w]`;
    /// the node's value is kept bit-exact when the placement does not turn.
    pub fn apply_rotation(self, rotation: [f32; 4]) -> [f32; 4] {
        if self.rotation == Self::IDENTITY.rotation {
            return rotation;
        }
        (self.quat() * glam::Quat::from_array(rotation))
            .normalize()
            .to_array()
    }
    /// The inverse mapping (for view helpers such as the floor height).
    pub fn unapply(self, point: [f32; 3]) -> [f32; 3] {
        let [x, y, z] = point;
        let [tx, ty, tz] = self.translation;
        let back = glam::Vec3::new(
            (x - tx) / self.scale,
            (y - ty) / self.scale,
            (z - tz) / self.scale,
        );
        (self.quat().inverse() * back).to_array()
    }
    /// This placement followed by scaling `factor` about `pivot` and moving
    /// by `offset` (world units): `x'' = factor * (x' - pivot) + pivot + offset`.
    pub fn then(self, pivot: [f32; 3], factor: f32, offset: [f32; 3]) -> Self {
        self.transformed(pivot, factor, Self::IDENTITY.rotation, offset)
    }
    /// This placement followed by `x'' = factor * turn(x' - pivot) + pivot +
    /// offset`, where `turn` is a unit quaternion (rotation about `pivot`).
    pub fn transformed(
        self,
        pivot: [f32; 3],
        factor: f32,
        turn: [f32; 4],
        offset: [f32; 3],
    ) -> Self {
        use glam::{Quat, Vec3};
        let q = Quat::from_array(turn);
        let rotation = if q == Quat::IDENTITY {
            self.quat()
        } else {
            (q * self.quat()).normalize()
        };
        let pivot = Vec3::from_array(pivot);
        let moved = q * (Vec3::from_array(self.translation) - pivot);
        let shift = |axis: usize| factor * moved[axis] + pivot[axis] + offset[axis];
        Self {
            scale: self.scale * factor,
            rotation: rotation.to_array(),
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
        self.require_editable()?;
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
                translation: [f32::INFINITY, 0., 0.],
                ..Placement::IDENTITY
            },
            Placement {
                translation: [0., 2e12, 0.],
                ..Placement::IDENTITY
            },
            Placement {
                rotation: [0., 0., 0., 2.],
                ..Placement::IDENTITY
            },
            Placement {
                rotation: [f32::NAN, 0., 0., 1.],
                ..Placement::IDENTITY
            },
        ] {
            assert!(!bad.valid(), "{bad:?}");
        }
    }
    #[test]
    fn rotation_turns_about_the_pivot_and_round_trips() {
        let quarter = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2).to_array();
        let p = Placement::IDENTITY
            .then([0.; 3], 2., [1., 0., 0.])
            .transformed([1., 0., 0.], 1., quarter, [0.; 3]);
        assert!(p.valid());
        // The pivot stays; +X from it turns to -Z.
        let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
        assert!(
            near(p.apply([0.; 3]), [1., 0., 0.]),
            "{:?}",
            p.apply([0.; 3])
        );
        assert!(
            near(p.apply([1., 0., 0.]), [1., 0., -2.]),
            "{:?}",
            p.apply([1., 0., 0.])
        );
        let m = p.matrix().transform_point3(glam::Vec3::new(1., 0., 0.));
        assert!(near(m.to_array(), [1., 0., -2.]));
        assert!(near(p.unapply(p.apply([3., -1., 2.])), [3., -1., 2.]));
        assert_eq!(
            Placement::IDENTITY.then([5.; 3], 1., [0.; 3]),
            Placement::IDENTITY
        );
    }
}
