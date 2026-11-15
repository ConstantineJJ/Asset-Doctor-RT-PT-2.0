use crate::{context::GpuResult, gpu::Gpu};
use ash::vk;
use std::sync::Arc;

pub(crate) fn memory_type(gpu: &Gpu, bits: u32, flags: vk::MemoryPropertyFlags) -> GpuResult<u32> {
    // SAFETY: Read-only physical-device query on a live instance.
    let memory = unsafe {
        gpu.surface
            .context
            .instance
            .get_physical_device_memory_properties(gpu.physical)
    };
    (0..memory.memory_type_count)
        .find(|&i| {
            bits & (1 << i) != 0
                && memory.memory_types[i as usize]
                    .property_flags
                    .contains(flags)
        })
        .ok_or_else(|| "No compatible GPU memory type".into())
}

pub(crate) struct Buffer {
    pub handle: vk::Buffer,
    memory: vk::DeviceMemory,
    gpu: Arc<Gpu>,
    size: usize,
}
fn validate_write(size: usize, length: usize) -> GpuResult<()> {
    if length == 0 || size != length {
        return Err("GPU buffer overwrite must match its nonzero allocation size".into());
    }
    Ok(())
}
impl Buffer {
    pub fn upload(gpu: Arc<Gpu>, bytes: &[u8], usage: vk::BufferUsageFlags) -> GpuResult<Self> {
        if bytes.is_empty() {
            return Err("Cannot upload an empty GPU buffer".into());
        }
        let mut buffer = Self {
            handle: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
            gpu,
            size: bytes.len(),
        };
        // SAFETY: Self owns each allocation before the next fallible operation.
        // Mapped coherent memory has at least bytes.len initialized input bytes.
        unsafe {
            buffer.handle = buffer.gpu.device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(bytes.len() as u64)
                    .usage(usage)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?;
            let req = buffer
                .gpu
                .device
                .get_buffer_memory_requirements(buffer.handle);
            let index = memory_type(
                &buffer.gpu,
                req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?;
            buffer.memory = buffer.gpu.device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(index),
                None,
            )?;
            buffer
                .gpu
                .device
                .bind_buffer_memory(buffer.handle, buffer.memory, 0)?;
            buffer.overwrite(bytes)?;
        }
        Ok(buffer)
    }
    /// # Safety
    /// Caller must retire every GPU access before overwriting this coherent buffer.
    pub unsafe fn overwrite(&self, bytes: &[u8]) -> GpuResult<()> {
        validate_write(self.size, bytes.len())?;
        // SAFETY: Allocation is HOST_VISIBLE|COHERENT, bounds checked above;
        // caller guarantees no device access while mapped/copied.
        unsafe {
            let target = self.gpu.device.map_memory(
                self.memory,
                0,
                self.size as u64,
                vk::MemoryMapFlags::empty(),
            )?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
            self.gpu.device.unmap_memory(self.memory);
        }
        Ok(())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: Renderer retires submissions before releasing scene buffers;
        // failed candidates were never submitted. Buffer precedes its memory.
        unsafe {
            self.gpu.device.destroy_buffer(self.handle, None);
            self.gpu.device.free_memory(self.memory, None);
        }
    }
}

pub(crate) struct DepthTarget {
    pub view: vk::ImageView,
    image: vk::Image,
    memory: vk::DeviceMemory,
    gpu: Arc<Gpu>,
}
impl DepthTarget {
    pub fn new(gpu: Arc<Gpu>, extent: vk::Extent2D) -> GpuResult<Self> {
        let mut target = Self {
            view: vk::ImageView::null(),
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            gpu,
        };
        // SAFETY: Live device, valid nonzero swapchain extent; owner cleans every
        // successful allocation on later failure. D32 feature checked explicitly.
        unsafe {
            let props = target
                .gpu
                .surface
                .context
                .instance
                .get_physical_device_format_properties(target.gpu.physical, vk::Format::D32_SFLOAT);
            if !props
                .optimal_tiling_features
                .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
            {
                return Err("GPU does not support D32 depth attachment".into());
            }
            target.image = target.gpu.device.create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(vk::Format::D32_SFLOAT)
                    .extent(vk::Extent3D {
                        width: extent.width,
                        height: extent.height,
                        depth: 1,
                    })
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .tiling(vk::ImageTiling::OPTIMAL)
                    .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                    .sharing_mode(vk::SharingMode::EXCLUSIVE),
                None,
            )?;
            let req = target
                .gpu
                .device
                .get_image_memory_requirements(target.image);
            let index = memory_type(
                &target.gpu,
                req.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;
            target.memory = target.gpu.device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(index),
                None,
            )?;
            target
                .gpu
                .device
                .bind_image_memory(target.image, target.memory, 0)?;
            target.view = target.gpu.device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(target.image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(vk::Format::D32_SFLOAT)
                    .subresource_range(
                        vk::ImageSubresourceRange::default()
                            .aspect_mask(vk::ImageAspectFlags::DEPTH)
                            .level_count(1)
                            .layer_count(1),
                    ),
                None,
            )?;
        }
        Ok(target)
    }
}
impl Drop for DepthTarget {
    fn drop(&mut self) {
        // SAFETY: Framebuffers/queued draws are retired by Swapchain owner first.
        unsafe {
            self.gpu.device.destroy_image_view(self.view, None);
            self.gpu.device.destroy_image(self.image, None);
            self.gpu.device.free_memory(self.memory, None);
        }
    }
}

#[cfg(test)]
mod buffer_tests {
    use super::*;
    #[test]
    fn coherent_overwrite_refuses_partial_empty_or_overlong_input_before_mapping() {
        assert!(validate_write(96, 96).is_ok());
        for size in [0, 95, 97] {
            assert!(validate_write(96, size).is_err());
        }
    }
}
