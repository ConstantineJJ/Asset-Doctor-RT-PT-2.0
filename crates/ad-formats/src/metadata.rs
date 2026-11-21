//! Bounded strict metadata for changed exports; viewing policy remains separate.
use ad_document::{AssetError, AssetResult, Cancellation, ErrorCode};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

pub(crate) fn unsupported() -> AssetError {
    AssetError::new(
        ErrorCode::Unsupported,
        "Changed export requires certified core metadata without opaque extras/extensions or unknown fields",
    )
}
struct Context<'a> {
    nodes: usize,
    max: usize,
    code: Option<ErrorCode>,
    cancel: &'a Cancellation,
}
struct Seed<'a, 'b> {
    ctx: &'a mut Context<'b>,
    depth: usize,
}
impl<'de> DeserializeSeed<'de> for Seed<'_, '_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        if self.ctx.cancel.check().is_err() {
            self.ctx.code = Some(ErrorCode::Cancelled);
            return Err(de::Error::custom("cancelled"));
        }
        if self.depth > 64 || self.ctx.nodes >= self.ctx.max {
            self.ctx.code = Some(ErrorCode::ResourceLimit);
            return Err(de::Error::custom("metadata complexity limit"));
        }
        self.ctx.nodes += 1;
        d.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_, '_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("bounded JSON value")
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite JSON"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.into()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(v) = seq.next_element_seed(Seed {
            ctx: self.ctx,
            depth: self.depth + 1,
        })? {
            values.try_reserve(1).map_err(|_| {
                self.ctx.code = Some(ErrorCode::ResourceLimit);
                de::Error::custom("metadata allocation")
            })?;
            values.push(v);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                self.ctx.code = Some(ErrorCode::Unsupported);
                return Err(de::Error::custom("duplicate metadata member"));
            }
            let value = map.next_value_seed(Seed {
                ctx: self.ctx,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}
pub(crate) fn parse(
    bytes: &[u8],
    max: usize,
    cancel: &Cancellation,
) -> AssetResult<(Value, usize)> {
    let mut ctx = Context {
        nodes: 0,
        max,
        code: None,
        cancel,
    };
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let result = Seed {
        ctx: &mut ctx,
        depth: 0,
    }
    .deserialize(&mut parser)
    .and_then(|v| {
        parser.end()?;
        Ok(v)
    });
    result.map(|v| (v, ctx.nodes)).map_err(|e| {
        AssetError::new(
            ctx.code.unwrap_or(ErrorCode::InvalidFormat),
            format!("Export metadata refused: {e}"),
        )
    })
}

#[derive(Clone, Copy)]
enum Kind {
    Root,
    Asset,
    Buffer,
    View,
    Accessor,
    Mesh,
    Primitive,
    Node,
    Scene,
    Material,
    Pbr,
    Info,
    Normal,
    Occlusion,
    Texture,
    Sampler,
    Image,
    Camera,
    Perspective,
    Orthographic,
}
fn keys(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Root => &[
            "asset",
            "scene",
            "scenes",
            "nodes",
            "meshes",
            "buffers",
            "bufferViews",
            "accessors",
            "materials",
            "textures",
            "images",
            "samplers",
            "cameras",
            "skins",
            "animations",
            "extensionsUsed",
            "extensionsRequired",
        ],
        Kind::Asset => &["version", "minVersion", "generator", "copyright"],
        Kind::Buffer => &["byteLength", "uri", "name"],
        Kind::View => &[
            "buffer",
            "byteOffset",
            "byteLength",
            "byteStride",
            "target",
            "name",
        ],
        Kind::Accessor => &[
            "bufferView",
            "byteOffset",
            "componentType",
            "normalized",
            "count",
            "type",
            "max",
            "min",
            "sparse",
            "name",
        ],
        Kind::Mesh => &["primitives", "weights", "name"],
        Kind::Primitive => &["attributes", "indices", "material", "mode", "targets"],
        Kind::Node => &[
            "camera",
            "children",
            "skin",
            "matrix",
            "mesh",
            "rotation",
            "scale",
            "translation",
            "weights",
            "name",
        ],
        Kind::Scene => &["nodes", "name"],
        Kind::Material => &[
            "name",
            "pbrMetallicRoughness",
            "normalTexture",
            "occlusionTexture",
            "emissiveTexture",
            "emissiveFactor",
            "alphaMode",
            "alphaCutoff",
            "doubleSided",
        ],
        Kind::Pbr => &[
            "baseColorFactor",
            "baseColorTexture",
            "metallicFactor",
            "roughnessFactor",
            "metallicRoughnessTexture",
        ],
        Kind::Info => &["index", "texCoord"],
        Kind::Normal => &["index", "texCoord", "scale"],
        Kind::Occlusion => &["index", "texCoord", "strength"],
        Kind::Texture => &["sampler", "source", "name"],
        Kind::Sampler => &["magFilter", "minFilter", "wrapS", "wrapT", "name"],
        Kind::Image => &["uri", "mimeType", "bufferView", "name"],
        Kind::Camera => &["orthographic", "perspective", "type", "name"],
        Kind::Perspective => &["aspectRatio", "yfov", "zfar", "znear"],
        Kind::Orthographic => &["xmag", "ymag", "zfar", "znear"],
    }
}
fn check(value: &Value, kind: Kind, cancel: &Cancellation) -> AssetResult<()> {
    cancel.check()?;
    let object = value.as_object().ok_or_else(unsupported)?;
    if matches!(kind, Kind::Accessor) {
        // gltf-json deliberately leaves min/max untyped. They cannot be opaque
        // containers in a changed export, even when viewing accepted the source.
        let components = match object.get("type").and_then(Value::as_str) {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4" | "MAT2") => 4,
            Some("MAT3") => 9,
            Some("MAT4") => 16,
            _ => return Err(unsupported()),
        };
        let component_type = object
            .get("componentType")
            .and_then(Value::as_u64)
            .ok_or_else(unsupported)?;
        for key in ["min", "max"] {
            if let Some(bound) = object.get(key) {
                let values = bound.as_array().ok_or_else(unsupported)?;
                if values.len() != components {
                    return Err(unsupported());
                }
                for v in values {
                    let n = v.as_f64().ok_or_else(unsupported)?;
                    let valid = match component_type {
                        5120 => n.fract() == 0. && (-128. ..=127.).contains(&n),
                        5121 => n.fract() == 0. && (0. ..=255.).contains(&n),
                        5122 => n.fract() == 0. && (-32768. ..=32767.).contains(&n),
                        5123 => n.fract() == 0. && (0. ..=65535.).contains(&n),
                        5125 => n.fract() == 0. && (0. ..=u32::MAX as f64).contains(&n),
                        5126 => n.is_finite() && n.abs() <= f32::MAX as f64,
                        _ => false,
                    };
                    if !valid {
                        return Err(unsupported());
                    }
                }
            }
        }
    }
    for (key, v) in object {
        if key == "extras" {
            if !v.is_null() {
                return Err(unsupported());
            }
            continue;
        }
        if key == "extensions" {
            if !v.as_object().is_some_and(Map::is_empty) {
                return Err(unsupported());
            }
            continue;
        }
        if !keys(kind).contains(&key.as_str()) {
            return Err(unsupported());
        }
        let child = match (kind, key.as_str()) {
            (Kind::Root, "asset") => Some((Kind::Asset, false)),
            (Kind::Root, "meshes") => Some((Kind::Mesh, true)),
            (Kind::Root, "buffers") => Some((Kind::Buffer, true)),
            (Kind::Root, "bufferViews") => Some((Kind::View, true)),
            (Kind::Root, "accessors") => Some((Kind::Accessor, true)),
            (Kind::Root, "nodes") => Some((Kind::Node, true)),
            (Kind::Root, "scenes") => Some((Kind::Scene, true)),
            (Kind::Root, "materials") => Some((Kind::Material, true)),
            (Kind::Root, "textures") => Some((Kind::Texture, true)),
            (Kind::Root, "samplers") => Some((Kind::Sampler, true)),
            (Kind::Root, "images") => Some((Kind::Image, true)),
            (Kind::Root, "cameras") => Some((Kind::Camera, true)),
            (Kind::Mesh, "primitives") => Some((Kind::Primitive, true)),
            (Kind::Material, "pbrMetallicRoughness") => Some((Kind::Pbr, false)),
            (Kind::Material, "normalTexture") => Some((Kind::Normal, false)),
            (Kind::Material, "occlusionTexture") => Some((Kind::Occlusion, false)),
            (Kind::Material, "emissiveTexture")
            | (Kind::Pbr, "baseColorTexture" | "metallicRoughnessTexture") => {
                Some((Kind::Info, false))
            }
            (Kind::Camera, "perspective") => Some((Kind::Perspective, false)),
            (Kind::Camera, "orthographic") => Some((Kind::Orthographic, false)),
            (Kind::Root, "extensionsUsed" | "extensionsRequired" | "skins" | "animations")
            | (Kind::Mesh | Kind::Node, "weights")
            | (Kind::Primitive, "targets") => {
                if !v.as_array().is_some_and(Vec::is_empty) {
                    return Err(unsupported());
                }
                None
            }
            (Kind::Accessor, "sparse") => {
                if !v.is_null() {
                    return Err(unsupported());
                }
                None
            }
            _ => None,
        };
        if let Some((child, array)) = child {
            if array {
                for item in v.as_array().ok_or_else(unsupported)? {
                    check(item, child, cancel)?;
                }
            } else {
                check(v, child, cancel)?;
            }
        }
    }
    Ok(())
}
/// ADR-0041 amendment: a placement-only export rewrites the root node
/// transforms and keeps every other member verbatim, so extras, extensions
/// and unknown fields are allowed. Animations and skins are refused: they can
/// drive or depend on the root transforms.
pub(crate) fn placement_only(value: &Value, cancel: &Cancellation) -> AssetResult<()> {
    cancel.check()?;
    let object = value.as_object().ok_or_else(unsupported)?;
    for key in ["animations", "skins"] {
        if object
            .get(key)
            .is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty))
        {
            return Err(AssetError::new(
                ErrorCode::Unsupported,
                "Moving a model with animations or skins is not supported yet",
            ));
        }
    }
    if !object.get("nodes").is_some_and(Value::is_array) {
        return Err(unsupported());
    }
    Ok(())
}
pub(crate) fn core_only(value: &Value, cancel: &Cancellation) -> AssetResult<()> {
    check(value, Kind::Root, cancel)
}
