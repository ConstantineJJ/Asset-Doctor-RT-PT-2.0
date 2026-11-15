use ad_render_vulkan::context::GpuResult;
use winit::window::Window;

#[cfg(windows)]
pub fn request_close(window: &Window) -> GpuResult<()> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    let handle = window.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("Expected Win32 window".into());
    };
    // SAFETY: Window keeps HWND live; posted WM_CLOSE follows the same event
    // path as the title-bar close button. No borrowed pointer payload is posted.
    if unsafe { PostMessageW(handle.hwnd.get(), WM_CLOSE, 0, 0) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn request_close(_window: &Window) -> GpuResult<()> {
    Err("Native smoke currently targets Windows".into())
}

pub fn show_error(message: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "Asset Doctor RT-PT 2.0"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: NUL-terminated UTF-16 buffers live for the modal call.
        unsafe {
            MessageBoxW(0, text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
        }
    }
}
