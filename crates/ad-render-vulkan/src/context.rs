use ash::{Entry, Instance, vk};
use std::ffi::CStr;
use std::io::Write;
use std::sync::atomic::{AtomicU32, Ordering};

pub type GpuResult<T> = Result<T, Box<dyn std::error::Error>>;

pub struct Context {
    // Kept alive until after the last Vulkan call (Drop destroys instance first).
    pub entry: Entry,
    pub instance: Instance,
    pub validation: bool,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    errors: Box<AtomicU32>,
}

unsafe extern "system" fn debug_message(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    user: *mut std::ffi::c_void,
) -> vk::Bool32 {
    // SAFETY: Vulkan invokes this callback with live callback data and the boxed
    // AtomicU32 that Context retains until the messenger has been destroyed.
    unsafe {
        if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
            (*(user as *const AtomicU32)).fetch_add(1, Ordering::Relaxed);
        }
        if !data.is_null() && !(*data).p_message.is_null() {
            let _ = writeln!(
                std::io::stderr().lock(),
                "validation {severity:?}: {}",
                CStr::from_ptr((*data).p_message).to_string_lossy()
            );
        }
    }
    vk::FALSE
}

impl Context {
    pub fn new(required_extensions: &[&CStr], require_validation: bool) -> GpuResult<Self> {
        // SAFETY: Entry owns the dynamically loaded Vulkan loader until all
        // dependent objects have been destroyed by Context's Drop.
        let entry = unsafe { Entry::load()? };
        // SAFETY: Read-only enumeration uses no externally owned Vulkan handles.
        let layers = unsafe { entry.enumerate_instance_layer_properties()? };
        let validation = layers.iter().any(|layer| {
            // SAFETY: layer_name is a NUL-terminated Vulkan fixed-size string.
            unsafe { CStr::from_ptr(layer.layer_name.as_ptr()) == c"VK_LAYER_KHRONOS_validation" }
        });
        if require_validation && !validation {
            return Err(
                "Vulkan validation layers are required but unavailable. Install LunarG Vulkan SDK."
                    .into(),
            );
        }
        let mut extensions: Vec<_> = required_extensions
            .iter()
            .map(|name| name.as_ptr())
            .collect();
        if validation {
            extensions.push(ash::ext::debug_utils::NAME.as_ptr());
        }
        let enabled_layers = if validation {
            vec![c"VK_LAYER_KHRONOS_validation".as_ptr()]
        } else {
            vec![]
        };
        let app = vk::ApplicationInfo::default()
            .application_name(c"Asset Doctor RT-PT 2.0 — GPU experiment")
            .application_version(vk::make_api_version(0, 0, 1, 0))
            .engine_name(c"Independent Vulkan spike")
            .api_version(vk::API_VERSION_1_1);
        let create = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&extensions)
            .enabled_layer_names(&enabled_layers);
        // SAFETY: All input pointers reference stable slices/C strings for this call.
        let instance = unsafe { entry.create_instance(&create, None)? };
        let mut context = Self {
            entry,
            instance,
            validation,
            debug: None,
            errors: Box::new(AtomicU32::new(0)),
        };
        if validation {
            let debug = ash::ext::debug_utils::Instance::new(&context.entry, &context.instance);
            let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .message_severity(
                    vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                        | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                )
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .pfn_user_callback(Some(debug_message))
                .user_data((&*context.errors as *const AtomicU32).cast_mut().cast());
            // SAFETY: Instance and callback user data outlive the messenger.
            let messenger = unsafe { debug.create_debug_utils_messenger(&info, None)? };
            context.debug = Some((debug, messenger));
        }
        eprintln!("validation_enabled={validation}");
        Ok(context)
    }

    pub fn validation_errors(&self) -> u32 {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn describe_devices(&self) -> GpuResult<()> {
        // SAFETY: Read-only queries on a live instance and its physical devices.
        unsafe {
            let devices = self.instance.enumerate_physical_devices()?;
            if devices.is_empty() {
                return Err("No Vulkan physical device available".into());
            }
            for gpu in devices {
                let props = self.instance.get_physical_device_properties(gpu);
                let name = CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy();
                eprintln!(
                    "gpu={name}; api={}.{}.{}; driver_raw={}; type={:?}",
                    vk::api_version_major(props.api_version),
                    vk::api_version_minor(props.api_version),
                    vk::api_version_patch(props.api_version),
                    props.driver_version,
                    props.device_type
                );
                let memory = self.instance.get_physical_device_memory_properties(gpu);
                for heap in &memory.memory_heaps[..memory.memory_heap_count as usize] {
                    eprintln!("memory_heap_bytes={} flags={:?}", heap.size, heap.flags);
                }
                let extensions = self.instance.enumerate_device_extension_properties(gpu)?;
                for name in [
                    ash::khr::swapchain::NAME,
                    ash::khr::acceleration_structure::NAME,
                    ash::khr::ray_tracing_pipeline::NAME,
                    ash::khr::ray_query::NAME,
                ] {
                    let found = extensions
                        .iter()
                        .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name);
                    eprintln!("extension {}={found}", name.to_string_lossy());
                }
                for (i, queue) in self
                    .instance
                    .get_physical_device_queue_family_properties(gpu)
                    .iter()
                    .enumerate()
                {
                    eprintln!(
                        "queue_family={i}; count={}; flags={:?}",
                        queue.queue_count, queue.queue_flags
                    );
                }
            }
        }
        Ok(())
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: All surfaces/devices are dropped by their owner before Context.
        // Debug messenger is destroyed before its instance and callback storage.
        unsafe {
            if let Some((debug, messenger)) = self.debug.take() {
                debug.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
