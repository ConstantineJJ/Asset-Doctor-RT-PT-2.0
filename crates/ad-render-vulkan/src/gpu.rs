use crate::context::{Context, GpuResult};
use ash::{Device, vk};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{ffi::CStr, sync::Arc};

/// Keeps the native window/display alive without depending on a GUI library.
pub trait SurfaceTarget: HasDisplayHandle + HasWindowHandle + Send + Sync {}
impl<T: HasDisplayHandle + HasWindowHandle + Send + Sync> SurfaceTarget for T {}

pub(crate) struct Surface {
    pub context: Arc<Context>,
    pub loader: ash::khr::surface::Instance,
    pub handle: vk::SurfaceKHR,
    pub maintenance_available: bool,
    _target: Arc<dyn SurfaceTarget>,
}

impl Surface {
    fn new(target: Arc<dyn SurfaceTarget>, validation: bool) -> GpuResult<Self> {
        let display = target
            .display_handle()
            .map_err(|error| format!("Display handle unavailable: {error}"))?
            .as_raw();
        let window = target
            .window_handle()
            .map_err(|error| format!("Window handle unavailable: {error}"))?
            .as_raw();
        let mut names: Vec<&CStr> = ash_window::enumerate_required_extensions(display)?
            .iter()
            .map(|name| {
                // SAFETY: ash-window returns pointers to static Vulkan extension strings.
                unsafe { CStr::from_ptr(*name) }
            })
            .collect();
        // SAFETY: Loader query owns no resources; entry remains live during query.
        let available = unsafe {
            let entry = ash::Entry::load()?;
            entry.enumerate_instance_extension_properties(None)?
        };
        let has = |name: &CStr| {
            available.iter().any(|ext| {
                // SAFETY: Vulkan returns NUL-terminated extension names.
                unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) == name }
            })
        };
        let maintenance_available = has(ash::ext::surface_maintenance1::NAME)
            && has(ash::khr::get_surface_capabilities2::NAME);
        if maintenance_available {
            names.extend([
                ash::ext::surface_maintenance1::NAME,
                ash::khr::get_surface_capabilities2::NAME,
            ]);
        }
        let context = Arc::new(Context::new(&names, validation)?);
        let loader = ash::khr::surface::Instance::new(&context.entry, &context.instance);
        // SAFETY: Arc target holds matching display/window handles alive through
        // surface destruction, and Context holds instance/loader until then.
        let handle = unsafe {
            ash_window::create_surface(&context.entry, &context.instance, display, window, None)?
        };
        Ok(Self {
            context,
            loader,
            handle,
            maintenance_available,
            _target: target,
        })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: Device/swapchain owners release their resources before Surface;
        // Context and target Arc fields remain live throughout this call.
        unsafe {
            self.loader.destroy_surface(self.handle, None);
        }
    }
}

pub(crate) struct Gpu {
    pub surface: Surface,
    pub physical: vk::PhysicalDevice,
    pub device: Device,
    pub queue: vk::Queue,
    pub family: u32,
}

impl Gpu {
    pub fn new(target: Arc<dyn SurfaceTarget>, validation: bool) -> GpuResult<Arc<Self>> {
        let surface = Surface::new(target, validation)?;
        let instance = &surface.context.instance;
        // SAFETY: All queries are on live instance/surface/physical devices;
        // device create slices and feature chains live until create completes.
        unsafe {
            let mut candidates = Vec::new();
            for physical in instance.enumerate_physical_devices()? {
                let extensions = instance.enumerate_device_extension_properties(physical)?;
                let has = |name: &CStr| {
                    extensions
                        .iter()
                        .any(|ext| CStr::from_ptr(ext.extension_name.as_ptr()) == name)
                };
                if !has(ash::khr::swapchain::NAME) {
                    continue;
                }
                for (family, props) in instance
                    .get_physical_device_queue_family_properties(physical)
                    .iter()
                    .enumerate()
                {
                    if props.queue_count > 0
                        && props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        && surface.loader.get_physical_device_surface_support(
                            physical,
                            family as u32,
                            surface.handle,
                        )?
                    {
                        let props = instance.get_physical_device_properties(physical);
                        candidates.push((
                            props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU,
                            physical,
                            family as u32,
                            surface.maintenance_available
                                && has(ash::ext::swapchain_maintenance1::NAME),
                        ));
                        break;
                    }
                }
            }
            candidates.sort_by_key(|item| !item.0);
            let (_, physical, family, extension_available) = candidates
                .into_iter()
                .next()
                .ok_or("No Vulkan graphics/present queue with swapchain support")?;
            let mut maintenance = vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT::default();
            if extension_available {
                let mut features =
                    vk::PhysicalDeviceFeatures2::default().push_next(&mut maintenance);
                instance.get_physical_device_features2(physical, &mut features);
            }
            let present_fences =
                extension_available && maintenance.swapchain_maintenance1 == vk::TRUE;
            if !present_fences {
                return Err("This Vulkan lifecycle experiment requires VK_EXT_swapchain_maintenance1 presentation fences; unsupported GPU/driver. No asset data was changed.".into());
            }
            let priorities = [1.0];
            let queues = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&priorities)];
            let mut names = vec![ash::khr::swapchain::NAME.as_ptr()];
            if present_fences {
                names.push(ash::ext::swapchain_maintenance1::NAME.as_ptr());
            }
            let mut create = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queues)
                .enabled_extension_names(&names);
            if present_fences {
                create = create.push_next(&mut maintenance);
            }
            let device = instance.create_device(physical, &create, None)?;
            let queue = device.get_device_queue(family, 0);
            let props = instance.get_physical_device_properties(physical);
            eprintln!(
                "selected_gpu={}; queue_family={family}; present_fences={present_fences}",
                CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy()
            );
            Ok(Arc::new(Self {
                surface,
                physical,
                device,
                queue,
                family,
            }))
        }
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        // SAFETY: Last GPU Arc drops only after swapchains/frame resources.
        // Device-lost wait errors do not prevent legal device teardown.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
        }
    }
}
