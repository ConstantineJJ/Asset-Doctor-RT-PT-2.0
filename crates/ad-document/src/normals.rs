//! Derived viewport normals. The source mesh remains unchanged.
pub(crate) fn normal_matrix(world: glam::Mat4) -> Option<glam::DMat3> {
    let linear = glam::DMat3::from_cols(
        world.x_axis.truncate().as_dvec3(),
        world.y_axis.truncate().as_dvec3(),
        world.z_axis.truncate().as_dvec3(),
    );
    let determinant = linear.determinant();
    if !determinant.is_finite() || determinant == 0. {
        return None;
    }
    let matrix = linear.inverse().transpose();
    matrix.is_finite().then_some(matrix)
}
pub(crate) fn transform_normal(matrix: Option<glam::DMat3>, source: [f32; 3]) -> [f32; 3] {
    let Some(matrix) = matrix else {
        return [0.; 3];
    };
    let v = matrix * glam::DVec3::from_array(source.map(f64::from));
    let n = v.normalize_or_zero();
    if n.is_finite() {
        n.as_vec3().to_array()
    } else {
        [0.; 3]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_transpose_handles_nonuniform_mirrored_and_singular_scales() {
        let world = glam::Mat4::from_scale(glam::vec3(-2., 1., 0.5));
        let n = transform_normal(normal_matrix(world), [1., 1., 1.]);
        let expected = glam::dvec3(-0.5, 1., 2.).normalize();
        assert!((glam::DVec3::from_array(n.map(f64::from)) - expected).length() < 1e-6);
        assert_eq!(
            transform_normal(
                normal_matrix(glam::Mat4::from_scale(glam::vec3(1., 0., 1.))),
                [0., 1., 0.]
            ),
            [0.; 3]
        );
        assert_eq!(
            transform_normal(normal_matrix(glam::Mat4::IDENTITY), [0.; 3]),
            [0.; 3]
        );
        assert!(
            transform_normal(
                normal_matrix(glam::Mat4::from_scale(glam::Vec3::splat(1e-20))),
                [0., 1., 0.]
            )[1] > 0.99
        );
    }
}
