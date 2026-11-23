use ad_document::{AssetError, AssetResult, Cancellation, ErrorCode, Influence, NodeId, Skin};
use gltf::accessor::{DataType, Dimensions};

fn invalid(message: &str) -> AssetError {
    AssetError::new(ErrorCode::InvalidFormat, message)
}

pub(crate) fn budget() -> AssetError {
    AssetError::new(
        ErrorCode::ResourceLimit,
        "Skin data exceeds the decoded geometry budget",
    )
}

pub(crate) fn read_skin(
    skin: gltf::Skin<'_>,
    bin: &[u8],
    remaining: &mut usize,
    cancel: &Cancellation,
) -> AssetResult<Skin> {
    cancel.check()?;
    let count = skin.joints().len();
    let bytes = count
        .checked_mul(size_of::<NodeId>() + size_of::<[[f32; 4]; 4]>())
        .ok_or_else(budget)?;
    *remaining = remaining.checked_sub(bytes).ok_or_else(budget)?;
    if let Some(accessor) = skin.inverse_bind_matrices()
        && (accessor.data_type() != DataType::F32
            || accessor.dimensions() != Dimensions::Mat4
            || accessor.normalized()
            || accessor.count() < count)
    {
        return Err(invalid(
            "Inverse bind matrices must use F32 MAT4 and cover all joints",
        ));
    }
    let mut inverse_bind = Vec::new();
    inverse_bind
        .try_reserve_exact(count)
        .map_err(|_| budget())?;
    if skin.inverse_bind_matrices().is_some() {
        let matrices = skin
            .reader(|_| Some(bin))
            .read_inverse_bind_matrices()
            .ok_or_else(|| invalid("Cannot read inverse bind matrices"))?;
        for m in matrices.take(count) {
            cancel.check()?;
            inverse_bind.push(m);
        }
    } else {
        inverse_bind.resize(
            count,
            [
                [1., 0., 0., 0.],
                [0., 1., 0., 0.],
                [0., 0., 1., 0.],
                [0., 0., 0., 1.],
            ],
        );
    }
    Ok(Skin {
        name: skin.name().unwrap_or("Skin").into(),
        joints: skin.joints().map(|n| NodeId(n.index())).collect(),
        inverse_bind,
        skeleton: skin.skeleton().map(|n| NodeId(n.index())),
    })
}

pub(crate) fn read_influences(
    primitive: &gltf::Primitive<'_>,
    bin: &[u8],
    count: usize,
    cancel: &Cancellation,
) -> AssetResult<Vec<Influence>> {
    use gltf::Semantic::{Joints, Weights};
    if primitive
        .attributes()
        .any(|(attribute, _)| matches!(attribute, Joints(set) | Weights(set) if set != 0))
    {
        return Err(AssetError::new(
            ErrorCode::Unsupported,
            "This build supports only JOINTS_0 and WEIGHTS_0",
        ));
    }
    let joints = primitive
        .get(&Joints(0))
        .ok_or_else(|| invalid("Skinned primitive is missing JOINTS_0"))?;
    let weights = primitive
        .get(&Weights(0))
        .ok_or_else(|| invalid("Skinned primitive is missing WEIGHTS_0"))?;
    let weights_supported = match weights.data_type() {
        DataType::F32 => !weights.normalized(),
        DataType::U8 | DataType::U16 => weights.normalized(),
        _ => false,
    };
    if joints.dimensions() != Dimensions::Vec4
        || weights.dimensions() != Dimensions::Vec4
        || joints.count() != count
        || weights.count() != count
        || joints.normalized()
        || !matches!(joints.data_type(), DataType::U8 | DataType::U16)
        || !weights_supported
    {
        return Err(invalid("Invalid JOINTS_0/WEIGHTS_0 accessor layout"));
    }
    let reader = primitive.reader(|_| Some(bin));
    let joints = reader
        .read_joints(0)
        .ok_or_else(|| invalid("Cannot read joints"))?
        .into_u16();
    let weights = reader
        .read_weights(0)
        .ok_or_else(|| invalid("Cannot read weights"))?
        .into_f32();
    let mut influences = Vec::new();
    influences.try_reserve_exact(count).map_err(|_| budget())?;
    for (i, (joints, weights)) in joints.zip(weights).enumerate() {
        if i % 4096 == 0 {
            cancel.check()?;
        }
        influences.push(Influence { joints, weights });
    }
    Ok(influences)
}
