use ad_render_vulkan::context::GpuResult;
use winit::window::Window;

#[cfg(windows)]
pub fn choose_model(window: &Window) -> GpuResult<Option<std::path::PathBuf>> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows_sys::Win32::UI::Controls::Dialogs::*;
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("Expected Win32 file-dialog owner".into());
    };
    let filter: Vec<u16> = "GLB models (*.glb)\0*.glb\0\0".encode_utf16().collect();
    let title: Vec<u16> = "Open a static GLB model\0".encode_utf16().collect();
    let mut path = vec![0u16; 32768];
    // SAFETY: Native OPENFILENAME is zero-initializable; all UTF-16 pointers and
    // owner window remain alive until the modal API returns. No directory scope
    // or network resource fetching is inferred from choosing this one file.
    unsafe {
        let mut dialog: OPENFILENAMEW = std::mem::zeroed();
        dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        dialog.hwndOwner = handle.hwnd.get();
        dialog.lpstrFilter = filter.as_ptr();
        dialog.lpstrTitle = title.as_ptr();
        dialog.lpstrFile = path.as_mut_ptr();
        dialog.nMaxFile = path.len() as u32;
        dialog.Flags = OFN_EXPLORER
            | OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_NOCHANGEDIR
            | OFN_DONTADDTORECENT;
        if GetOpenFileNameW(&mut dialog) == 0 {
            let error = CommDlgExtendedError();
            if error == 0 {
                return Ok(None);
            }
            return Err(format!("Cannot open file dialog ({error})").into());
        }
    }
    let end = path
        .iter()
        .position(|&value| value == 0)
        .ok_or("File dialog returned an unterminated path")?;
    Ok(Some(OsString::from_wide(&path[..end]).into()))
}
#[cfg(not(windows))]
pub fn choose_model(_window: &Window) -> GpuResult<Option<std::path::PathBuf>> {
    Err("Native file picker currently targets Windows".into())
}

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
    show_message(message);
}
fn show_message(message: &str) {
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
