use crate::{SurfaceSize, context::GpuResult, gpu::Gpu, presentation};
use crate::{grid::GridPipeline, pipeline::MeshPipeline, resources::DepthTarget, sky::SkyPipeline};
use ash::vk;
use std::sync::Arc;

pub(crate) struct Swapchain {
    pub loader: ash::khr::swapchain::Device,
    pub handle: vk::SwapchainKHR,
    pub extent: vk::Extent2D,
    pub pass: vk::RenderPass,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub finished: Vec<vk::Semaphore>,
    pub pipeline: Option<MeshPipeline>,
    pub grid: Option<GridPipeline>,
    pub sky: Option<SkyPipeline>,
    pub sky_panorama: Option<SkyPipeline>,
    /// Object outline mask and pipelines (sized like the swapchain).
    pub outline: Option<crate::outline::OutlineTarget>,
    pub encode_srgb: bool,
    pub format: vk::Format,
    depth: Option<DepthTarget>,
    views: Vec<vk::ImageView>,
    gpu: Arc<Gpu>,
}

impl Swapchain {
    pub fn new(gpu: Arc<Gpu>, size: SurfaceSize) -> GpuResult<Option<Self>> {
        let surface = &gpu.surface;
        // SAFETY: Live physical device/surface; all temporary create-info slices
        // outlive calls. Partial initialization is owned by Self and cleaned on ?.
        unsafe {
            let caps = surface
                .loader
                .get_physical_device_surface_capabilities(gpu.physical, surface.handle)?;
            let Some(extent) = presentation::extent(size, &caps) else {
                return Ok(None);
            };
            if !caps
                .supported_usage_flags
                .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            {
                return Err("Surface cannot be used as a Vulkan color attachment".into());
            }
            let formats = surface
                .loader
                .get_physical_device_surface_formats(gpu.physical, surface.handle)?;
            let mut format =
                presentation::surface_format(&formats).ok_or("Surface has no formats")?;
            if format.format == vk::Format::UNDEFINED {
                format.format = vk::Format::B8G8R8A8_UNORM;
            }
            let alpha = [
                vk::CompositeAlphaFlagsKHR::OPAQUE,
                vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::INHERIT,
            ]
            .into_iter()
            .find(|alpha| caps.supported_composite_alpha.contains(*alpha))
            .ok_or("Surface has no composite alpha mode")?;
            let loader = ash::khr::swapchain::Device::new(&surface.context.instance, &gpu.device);
            let mut result = Self {
                loader,
                handle: vk::SwapchainKHR::null(),
                extent,
                pass: vk::RenderPass::null(),
                framebuffers: vec![],
                finished: vec![],
                pipeline: None,
                grid: None,
                sky: None,
                sky_panorama: None,
                outline: None,
                format: format.format,
                encode_srgb: !matches!(
                    format.format,
                    vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB
                ),
                depth: None,
                views: vec![],
                gpu,
            };
            let create = vk::SwapchainCreateInfoKHR::default()
                .surface(result.gpu.surface.handle)
                .min_image_count(presentation::image_count(&caps))
                .image_format(format.format)
                .image_color_space(format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(alpha)
                .present_mode(vk::PresentModeKHR::FIFO)
                .clipped(true);
            result.handle = result.loader.create_swapchain(&create, None)?;
            result.depth = Some(DepthTarget::new(result.gpu.clone(), extent)?);
            let attachment = [
                vk::AttachmentDescription::default()
                    .format(format.format)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .final_layout(vk::ImageLayout::PRESENT_SRC_KHR),
                vk::AttachmentDescription::default()
                    .format(vk::Format::D32_SFLOAT)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                    .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                    .initial_layout(vk::ImageLayout::UNDEFINED)
                    .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
            ];
            let refs = [vk::AttachmentReference {
                attachment: 0,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            }];
            let depth_ref = vk::AttachmentReference {
                attachment: 1,
                layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
            };
            let subpass = [vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&refs)
                .depth_stencil_attachment(&depth_ref)];
            let dependency = [vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                        | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                )
                .dst_stage_mask(
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                        | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                        | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                )
                .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .dst_access_mask(
                    vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                        | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                )];
            result.pass = result.gpu.device.create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachment)
                    .subpasses(&subpass)
                    .dependencies(&dependency),
                None,
            )?;
            result.pipeline = Some(MeshPipeline::new(result.gpu.clone(), result.pass)?);
            result.grid = Some(GridPipeline::new(result.gpu.clone(), result.pass)?);
            result.sky = Some(SkyPipeline::new(result.gpu.clone(), result.pass, false)?);
            result.sky_panorama = Some(SkyPipeline::new(result.gpu.clone(), result.pass, true)?);
            result.outline = Some(crate::outline::OutlineTarget::new(
                result.gpu.clone(),
                extent,
                result.pass,
            )?);
            for image in result.loader.get_swapchain_images(result.handle)? {
                let range = vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1);
                let view = result.gpu.device.create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.format)
                        .subresource_range(range),
                    None,
                )?;
                result.views.push(view);
                let views = [
                    view,
                    result
                        .depth
                        .as_ref()
                        .expect("Depth target created before framebuffer")
                        .view,
                ];
                let framebuffer = result.gpu.device.create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(result.pass)
                        .attachments(&views)
                        .width(extent.width)
                        .height(extent.height)
                        .layers(1),
                    None,
                )?;
                result.framebuffers.push(framebuffer);
                result.finished.push(
                    result
                        .gpu
                        .device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?,
                );
            }
            eprintln!(
                "swapchain={}x{}; images={}; format={:?}",
                extent.width,
                extent.height,
                result.views.len(),
                format.format
            );
            Ok(Some(result))
        }
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        // SAFETY: Renderer waits submit AND presentation fences before dropping
        // this private object. During failed initialization no work was submitted.
        // Children are destroyed before their parent render pass/swapchain/device.
        unsafe {
            for framebuffer in self.framebuffers.drain(..) {
                self.gpu.device.destroy_framebuffer(framebuffer, None);
            }
            self.grid = None;
            self.sky = None;
            self.sky_panorama = None;
            self.outline = None;
            self.pipeline = None;
            self.gpu.device.destroy_render_pass(self.pass, None);
            for view in self.views.drain(..) {
                self.gpu.device.destroy_image_view(view, None);
            }
            for semaphore in self.finished.drain(..) {
                self.gpu.device.destroy_semaphore(semaphore, None);
            }
            self.depth = None;
            self.loader.destroy_swapchain(self.handle, None);
        }
    }
}
