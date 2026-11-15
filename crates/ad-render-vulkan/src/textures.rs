use crate::{
    context::GpuResult,
    gpu::Gpu,
    resources::{Buffer, memory_type},
};
use ad_document::{
    DrawRange, Filter, ImportLimits, Mipmap, Texture, TextureAdmission, TextureEncoding,
    TextureImage, TextureSampler, TextureStats, Wrap,
};
use ash::vk;
use std::{collections::HashMap, sync::Arc};

pub(crate) const MATERIAL_CHANNELS: usize = 5;
const MAX_MATERIAL_SETS: usize = 4096;

pub(crate) struct TextureLayout {
    pub handle: vk::DescriptorSetLayout,
    gpu: Arc<Gpu>,
}
impl TextureLayout {
    pub fn new(gpu: Arc<Gpu>) -> GpuResult<Self> {
        let bindings: Vec<_> = (0..MATERIAL_CHANNELS as u32)
            .flat_map(|channel| {
                [
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(channel * 2)
                        .descriptor_count(1)
                        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                        .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(channel * 2 + 1)
                        .descriptor_count(1)
                        .descriptor_type(vk::DescriptorType::SAMPLER)
                        .stage_flags(vk::ShaderStageFlags::FRAGMENT),
                ]
            })
            .collect();
        // SAFETY: Bindings live through creation; owner retains the device.
        let handle = unsafe {
            gpu.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )?
        };
        Ok(Self { handle, gpu })
    }
}
impl Drop for TextureLayout {
    fn drop(&mut self) {
        // SAFETY: Pipeline/pool owners retire dependent use before releasing layout.
        unsafe {
            self.gpu
                .device
                .destroy_descriptor_set_layout(self.handle, None);
        }
    }
}

struct Upload {
    pool: vk::CommandPool,
    fence: vk::Fence,
    pending: bool,
    gpu: Arc<Gpu>,
}
impl Upload {
    fn new(gpu: Arc<Gpu>) -> GpuResult<Self> {
        let mut upload = Self {
            pool: vk::CommandPool::null(),
            fence: vk::Fence::null(),
            pending: false,
            gpu,
        };
        // SAFETY: Partial allocations are immediately owned and cleaned on failure.
        unsafe {
            upload.pool = upload.gpu.device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(upload.gpu.family)
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None,
            )?;
            upload.fence = upload
                .gpu
                .device
                .create_fence(&vk::FenceCreateInfo::default(), None)?;
        }
        Ok(upload)
    }
    fn submit(&mut self, command: vk::CommandBuffer) -> GpuResult<()> {
        // SAFETY: Single-threaded queue ownership, recorded private command buffer;
        // source staging/image allocations remain live until this fence retires.
        unsafe {
            self.gpu.device.end_command_buffer(command)?;
            let commands = [command];
            let submit = [vk::SubmitInfo::default().command_buffers(&commands)];
            self.gpu
                .device
                .queue_submit(self.gpu.queue, &submit, self.fence)?;
            self.pending = true;
            self.gpu
                .device
                .wait_for_fences(&[self.fence], true, u64::MAX)?;
            self.pending = false;
        }
        Ok(())
    }
}
impl Drop for Upload {
    fn drop(&mut self) {
        // SAFETY: Retire a submitted copy even if a later wait failed. On device loss
        // Vulkan permits destruction. Command buffers are released with their pool.
        unsafe {
            if self.pending {
                let _ = self.gpu.device.device_wait_idle();
            }
            self.gpu.device.destroy_fence(self.fence, None);
            self.gpu.device.destroy_command_pool(self.pool, None);
        }
    }
}

struct GpuImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    mips: u32,
    gpu: Arc<Gpu>,
}
fn barrier(
    device: &ash::Device,
    command: vk::CommandBuffer,
    image: vk::Image,
    range: vk::ImageSubresourceRange,
    layouts: (vk::ImageLayout, vk::ImageLayout),
    access: (vk::AccessFlags, vk::AccessFlags),
    stages: (vk::PipelineStageFlags, vk::PipelineStageFlags),
) {
    let barriers = [vk::ImageMemoryBarrier::default()
        .image(image)
        .subresource_range(range)
        .old_layout(layouts.0)
        .new_layout(layouts.1)
        .src_access_mask(access.0)
        .dst_access_mask(access.1)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)];
    // SAFETY: Caller records to a live command buffer with ranges/layouts tracking
    // only the new privately owned image. No queue-family ownership transfer.
    unsafe {
        device.cmd_pipeline_barrier(
            command,
            stages.0,
            stages.1,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &barriers,
        );
    }
}
impl GpuImage {
    fn upload(gpu: Arc<Gpu>, data: &TextureImage, encoding: TextureEncoding) -> GpuResult<Self> {
        data.validate(ImportLimits::default())?;
        let staging = Buffer::upload(gpu.clone(), &data.rgba, vk::BufferUsageFlags::TRANSFER_SRC)?;
        let mut result = Self {
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            view: vk::ImageView::null(),
            mips: 32 - data.width.max(data.height).leading_zeros(),
            gpu,
        };
        let device = &result.gpu.device;
        // SAFETY: Validated nonzero dimensions/byte length; every created handle
        // is owned before the next fallible call. Upload guard retires GPU copies.
        unsafe {
            let format = match encoding {
                TextureEncoding::Srgb => vk::Format::R8G8B8A8_SRGB,
                TextureEncoding::Linear => vk::Format::R8G8B8A8_UNORM,
            };
            let props = result
                .gpu
                .surface
                .context
                .instance
                .get_physical_device_format_properties(result.gpu.physical, format);
            let required = vk::FormatFeatureFlags::SAMPLED_IMAGE
                | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                | vk::FormatFeatureFlags::BLIT_SRC
                | vk::FormatFeatureFlags::BLIT_DST;
            if !props.optimal_tiling_features.contains(required) {
                return Err(format!(
                    "GPU does not support {format:?} sampling with linear mip generation"
                )
                .into());
            }
            result.image = device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(format)
                    .extent(vk::Extent3D {
                        width: data.width,
                        height: data.height,
                        depth: 1,
                    })
                    .mip_levels(result.mips)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(
                        vk::ImageUsageFlags::TRANSFER_DST
                            | vk::ImageUsageFlags::TRANSFER_SRC
                            | vk::ImageUsageFlags::SAMPLED,
                    )
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?;
            let req = device.get_image_memory_requirements(result.image);
            result.memory = device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(memory_type(
                        &result.gpu,
                        req.memory_type_bits,
                        vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    )?),
                None,
            )?;
            device.bind_image_memory(result.image, result.memory, 0)?;
            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(result.mips)
                .layer_count(1);
            result.view = device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(result.image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(format)
                    .subresource_range(range),
                None,
            )?;
            let mut upload = Upload::new(result.gpu.clone())?;
            let command = device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(upload.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?[0];
            device.begin_command_buffer(
                command,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            barrier(
                device,
                command,
                result.image,
                range,
                (
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                ),
                (vk::AccessFlags::empty(), vk::AccessFlags::TRANSFER_WRITE),
                (
                    vk::PipelineStageFlags::TOP_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                ),
            );
            device.cmd_copy_buffer_to_image(
                command,
                staging.handle,
                result.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::BufferImageCopy::default()
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: data.width,
                        height: data.height,
                        depth: 1,
                    })],
            );
            let (mut width, mut height) = (data.width as i32, data.height as i32);
            for level in 1..result.mips {
                let previous = vk::ImageSubresourceRange {
                    base_mip_level: level - 1,
                    level_count: 1,
                    ..range
                };
                barrier(
                    device,
                    command,
                    result.image,
                    previous,
                    (
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    ),
                    (
                        vk::AccessFlags::TRANSFER_WRITE,
                        vk::AccessFlags::TRANSFER_READ,
                    ),
                    (
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::TRANSFER,
                    ),
                );
                let (next_w, next_h) = ((width / 2).max(1), (height / 2).max(1));
                let sub = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1);
                let blit = [vk::ImageBlit::default()
                    .src_subresource(sub.mip_level(level - 1))
                    .src_offsets([
                        vk::Offset3D::default(),
                        vk::Offset3D {
                            x: width,
                            y: height,
                            z: 1,
                        },
                    ])
                    .dst_subresource(sub.mip_level(level))
                    .dst_offsets([
                        vk::Offset3D::default(),
                        vk::Offset3D {
                            x: next_w,
                            y: next_h,
                            z: 1,
                        },
                    ])];
                device.cmd_blit_image(
                    command,
                    result.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    result.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &blit,
                    vk::Filter::LINEAR,
                );
                barrier(
                    device,
                    command,
                    result.image,
                    previous,
                    (
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    ),
                    (vk::AccessFlags::TRANSFER_READ, vk::AccessFlags::SHADER_READ),
                    (
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                    ),
                );
                width = next_w;
                height = next_h;
            }
            barrier(
                device,
                command,
                result.image,
                vk::ImageSubresourceRange {
                    base_mip_level: result.mips - 1,
                    level_count: 1,
                    ..range
                },
                (
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ),
                (
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::AccessFlags::SHADER_READ,
                ),
                (
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                ),
            );
            upload.submit(command)?;
        }
        Ok(result)
    }
}
impl Drop for GpuImage {
    fn drop(&mut self) {
        // SAFETY: Candidate upload guard or scene renderer retires all using work.
        unsafe {
            self.gpu.device.destroy_image_view(self.view, None);
            self.gpu.device.destroy_image(self.image, None);
            self.gpu.device.free_memory(self.memory, None);
        }
    }
}
struct Sampler {
    handle: vk::Sampler,
    gpu: Arc<Gpu>,
}
impl Sampler {
    fn new(gpu: Arc<Gpu>, source: TextureSampler, mips: u32) -> GpuResult<Self> {
        let filter = |value| match value {
            Filter::Nearest => vk::Filter::NEAREST,
            Filter::Linear => vk::Filter::LINEAR,
        };
        let wrap = |value| match value {
            Wrap::Repeat => vk::SamplerAddressMode::REPEAT,
            Wrap::MirroredRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
            Wrap::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        };
        // SAFETY: No optional device features requested; all enums/LOD ranges valid.
        let handle = unsafe {
            gpu.device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .min_filter(filter(source.min))
                    .mag_filter(filter(source.mag))
                    .mipmap_mode(if source.mipmap == Mipmap::Linear {
                        vk::SamplerMipmapMode::LINEAR
                    } else {
                        vk::SamplerMipmapMode::NEAREST
                    })
                    .address_mode_u(wrap(source.u))
                    .address_mode_v(wrap(source.v))
                    .address_mode_w(vk::SamplerAddressMode::REPEAT)
                    .max_lod(if source.mipmap == Mipmap::None {
                        0.
                    } else {
                        (mips - 1) as f32
                    }),
                None,
            )?
        };
        Ok(Self { handle, gpu })
    }
}
impl Drop for Sampler {
    fn drop(&mut self) {
        // SAFETY: Scene retire/pool deletion precedes releasing sampled resources.
        unsafe {
            self.gpu.device.destroy_sampler(self.handle, None);
        }
    }
}

/// CPU-side plan shares source identity while keeping transfer roles independent.
struct TexturePlan {
    textures: Vec<(Arc<Texture>, TextureEncoding)>,
    draw_slots: Vec<usize>,
    materials: Vec<[usize; MATERIAL_CHANNELS]>,
    stats: TextureStats,
}
impl TexturePlan {
    fn new(draws: &[DrawRange], max_materials: usize) -> GpuResult<Self> {
        let mut admission = TextureAdmission::default();
        let white = Arc::new(Texture {
            image: Arc::new(TextureImage {
                width: 1,
                height: 1,
                rgba: Arc::from([255; 4]),
            }),
            sampler: TextureSampler::default(),
        });
        let mut textures = vec![
            (white.clone(), TextureEncoding::Srgb),
            (white, TextureEncoding::Linear),
        ];
        let mut lookup = HashMap::new();
        let mut draw_slots = Vec::with_capacity(draws.len());
        let mut materials = Vec::new();
        let mut material_lookup = HashMap::new();
        for draw in draws {
            let mut slots = [0, 1, 1, 1, 0];
            for (slot, (texture, encoding)) in slots.iter_mut().zip([
                (&draw.color_texture, TextureEncoding::Srgb),
                (&draw.metallic_roughness_texture, TextureEncoding::Linear),
                (&draw.normal_texture, TextureEncoding::Linear),
                (&draw.occlusion_texture, TextureEncoding::Linear),
                (&draw.emissive_texture, TextureEncoding::Srgb),
            ]) {
                if let Some(texture) = texture {
                    admission.admit(texture, encoding, ImportLimits::default())?;
                    *slot = *lookup
                        .entry((Arc::as_ptr(texture) as usize, encoding))
                        .or_insert_with(|| {
                            textures.push((texture.clone(), encoding));
                            textures.len() - 1
                        });
                }
            }
            let material = if let Some(index) = material_lookup.get(&slots) {
                *index
            } else {
                if materials.len() >= max_materials {
                    return Err("Scene exceeds material texture-combination budget".into());
                }
                let index = materials.len();
                materials.push(slots);
                material_lookup.insert(slots, index);
                index
            };
            draw_slots.push(material);
        }
        Ok(Self {
            textures,
            draw_slots,
            materials,
            stats: admission.stats,
        })
    }
}

pub(crate) struct TextureBindings {
    pub sets: Vec<vk::DescriptorSet>,
    pub draw_slots: Vec<usize>,
    pool: vk::DescriptorPool,
    layout: TextureLayout,
    images: Vec<GpuImage>,
    samplers: Vec<Sampler>,
    gpu: Arc<Gpu>,
}
impl TextureBindings {
    pub fn new(gpu: Arc<Gpu>, draws: &[DrawRange]) -> GpuResult<Self> {
        let TexturePlan {
            textures,
            draw_slots,
            materials,
            stats,
        } = TexturePlan::new(draws, MAX_MATERIAL_SETS)?;
        let mut result = Self {
            sets: vec![],
            draw_slots,
            pool: vk::DescriptorPool::null(),
            layout: TextureLayout::new(gpu.clone())?,
            images: vec![],
            samplers: vec![],
            gpu,
        };
        let mut images = HashMap::new();
        let mut image_slots = Vec::with_capacity(textures.len());
        for (texture, encoding) in &textures {
            let key = (Arc::as_ptr(&texture.image) as usize, *encoding);
            let slot = if let Some(slot) = images.get(&key) {
                *slot
            } else {
                let slot = result.images.len();
                result.images.push(GpuImage::upload(
                    result.gpu.clone(),
                    &texture.image,
                    *encoding,
                )?);
                images.insert(key, slot);
                slot
            };
            image_slots.push(slot);
        }
        for (index, (texture, _)) in textures.iter().enumerate() {
            result.samplers.push(Sampler::new(
                result.gpu.clone(),
                texture.sampler,
                result.images[image_slots[index]].mips,
            )?);
        }
        // SAFETY: Material tuples were capped before GPU allocation. All image,
        // sampler and create-info arrays stay live through synchronous updates.
        unsafe {
            let count = (materials.len() * MATERIAL_CHANNELS) as u32;
            let sizes = [
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLED_IMAGE,
                    descriptor_count: count,
                },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLER,
                    descriptor_count: count,
                },
            ];
            result.pool = result.gpu.device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(materials.len() as u32)
                    .pool_sizes(&sizes),
                None,
            )?;
            let layouts = vec![result.layout.handle; materials.len()];
            result.sets = result.gpu.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(result.pool)
                    .set_layouts(&layouts),
            )?;
            for (index, slots) in materials.iter().enumerate() {
                let infos: Vec<_> = slots
                    .iter()
                    .map(|&slot| vk::DescriptorImageInfo {
                        sampler: result.samplers[slot].handle,
                        image_view: result.images[image_slots[slot]].view,
                        image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    })
                    .collect();
                let writes: Vec<_> = infos
                    .iter()
                    .enumerate()
                    .flat_map(|(channel, info)| {
                        [
                            vk::WriteDescriptorSet::default()
                                .dst_set(result.sets[index])
                                .dst_binding(channel as u32 * 2)
                                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                                .image_info(std::slice::from_ref(info)),
                            vk::WriteDescriptorSet::default()
                                .dst_set(result.sets[index])
                                .dst_binding(channel as u32 * 2 + 1)
                                .descriptor_type(vk::DescriptorType::SAMPLER)
                                .image_info(std::slice::from_ref(info)),
                        ]
                    })
                    .collect();
                result.gpu.device.update_descriptor_sets(&writes, &[]);
            }
        }
        eprintln!("gpu_material_binding_sets={}", result.sets.len());
        eprintln!(
            "gpu_material_images={} material_textures={} mip_bytes={} decoded_images={} srgb_images={} linear_images={}",
            result.images.len() - 2,
            stats.textures,
            stats.mip_bytes,
            stats.images,
            images
                .keys()
                .filter(|(_, encoding)| *encoding == TextureEncoding::Srgb)
                .count()
                - 1,
            images
                .keys()
                .filter(|(_, encoding)| *encoding == TextureEncoding::Linear)
                .count()
                - 1
        );
        Ok(result)
    }
}
impl Drop for TextureBindings {
    fn drop(&mut self) {
        // SAFETY: Scene is retired; releasing pool invalidates sets before image data.
        unsafe {
            self.gpu.device.destroy_descriptor_pool(self.pool, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn descriptor_plan_shares_sources_but_separates_transfer_roles_and_white_fallbacks() {
        let texture = Arc::new(Texture {
            image: Arc::new(TextureImage {
                width: 2,
                height: 2,
                rgba: Arc::from([128; 16]),
            }),
            sampler: TextureSampler::default(),
        });
        let draw = |color, mr| DrawRange {
            index_start: 0,
            index_count: 3,
            base_color: [1.; 4],
            metallic_roughness: [1.; 2],
            color_texture: color,
            metallic_roughness_texture: mr,
            normal_texture: None,
            normal_scale: 1.,
            occlusion_texture: None,
            occlusion_strength: 1.,
            emissive_factor: [0.; 3],
            emissive_texture: None,
            alpha_cutoff: None,
        };
        let plan = TexturePlan::new(
            &[
                draw(Some(texture.clone()), Some(texture.clone())),
                draw(Some(texture.clone()), None),
                draw(None, Some(texture)),
            ],
            4096,
        )
        .unwrap();
        let mut normal_draw = draw(None, Some(plan.textures[3].0.clone()));
        normal_draw.normal_texture = normal_draw.metallic_roughness_texture.clone();
        normal_draw.occlusion_texture = normal_draw.metallic_roughness_texture.clone();
        let shared = TexturePlan::new(&[normal_draw], 4096).unwrap();
        assert_eq!(shared.draw_slots, [0]);
        assert_eq!(shared.materials, [[0, 2, 2, 2, 0]]);
        assert_eq!(
            (
                shared.textures.len(),
                shared.stats.sampled_images,
                shared.stats.mip_bytes
            ),
            (3, 1, 20)
        );
        assert_eq!(plan.draw_slots, [0, 1, 2]);
        assert_eq!(
            plan.materials,
            [[2, 3, 1, 1, 0], [2, 1, 1, 1, 0], [0, 3, 1, 1, 0]]
        );
        let mut emissive = draw(Some(plan.textures[2].0.clone()), None);
        emissive.emissive_texture = emissive.color_texture.clone();
        emissive.emissive_factor = [1.; 3];
        let emission = TexturePlan::new(&[emissive], 1).unwrap();
        assert_eq!(emission.materials, [[2, 1, 1, 1, 2]]);
        assert_eq!(emission.stats.sampled_images, 1);
        assert_eq!(plan.textures.len(), 4);
        let repeated = TexturePlan::new(
            &[
                draw(Some(plan.textures[2].0.clone()), None),
                draw(Some(plan.textures[2].0.clone()), None),
            ],
            1,
        )
        .unwrap();
        assert_eq!(repeated.draw_slots, [0, 0]);
        assert_eq!(repeated.materials.len(), 1);
        assert!(
            TexturePlan::new(
                &[
                    draw(Some(plan.textures[2].0.clone()), None),
                    draw(None, Some(plan.textures[3].0.clone()))
                ],
                1
            )
            .is_err()
        );
        assert_eq!(plan.textures[2].1, TextureEncoding::Srgb);
        assert_eq!(plan.textures[3].1, TextureEncoding::Linear);
        assert!(Arc::ptr_eq(
            &plan.textures[2].0.image,
            &plan.textures[3].0.image
        ));
        assert_eq!(
            (
                plan.stats.images,
                plan.stats.sampled_images,
                plan.stats.mip_bytes
            ),
            (1, 2, 40)
        );
    }
}
