use std::{fs, path::PathBuf};
fn main() {
    for (name, stage) in [
        ("mesh.vert", naga::ShaderStage::Vertex),
        ("mesh.frag", naga::ShaderStage::Fragment),
        ("grid.vert", naga::ShaderStage::Vertex),
        ("grid.frag", naga::ShaderStage::Fragment),
        ("sky.vert", naga::ShaderStage::Vertex),
        ("shadow.vert", naga::ShaderStage::Vertex),
        ("sky.frag", naga::ShaderStage::Fragment),
        ("sky_panorama.frag", naga::ShaderStage::Fragment),
    ] {
        let path = format!("shaders/{name}");
        println!("cargo:rerun-if-changed={path}");
        let source = fs::read_to_string(&path).expect("Read shader source");
        let module = naga::front::glsl::Frontend::default()
            .parse(
                &naga::front::glsl::Options {
                    stage,
                    defines: Default::default(),
                },
                &source,
            )
            .expect("Parse GLSL");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("Validate shader");
        let options = naga::back::spv::Options {
            flags: naga::back::spv::WriterFlags::LABEL_VARYINGS,
            fake_missing_bindings: false,
            // Set 0: frame uniform, shadow map and environment panorama
            // (mesh, floor) or panorama image/sampler (sky);
            // set 1: material images and samplers.
            binding_map: (0..5)
                .map(|binding| (0, binding))
                .chain((0..10).map(|binding| (1, binding)))
                .map(|(group, binding)| {
                    (
                        naga::ResourceBinding { group, binding },
                        naga::back::spv::BindingInfo {
                            descriptor_set: group,
                            binding,
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            ..Default::default()
        };
        let pipeline = naga::back::spv::PipelineOptions {
            shader_stage: stage,
            entry_point: "main".into(),
        };
        let words = naga::back::spv::write_vec(&module, &info, &options, Some(&pipeline))
            .expect("Generate SPIR-V");
        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        fs::write(
            PathBuf::from(std::env::var("OUT_DIR").unwrap()).join(format!("{name}.spv")),
            bytes,
        )
        .expect("Write compiled shader");
    }
}
