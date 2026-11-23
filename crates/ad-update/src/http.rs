//! HTTPS GET through WinHTTP (system TLS, certificates and proxy settings).
//! Redirects are followed by WinHTTP (HTTPS to HTTP is refused by default).
use crate::Result;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Networking::WinHttp::*;

/// Hosts a request may start at; asset downloads redirect from github.com
/// to GitHub's storage, which WinHTTP follows on its own.
const HOSTS: [&str; 2] = ["api.github.com", "github.com"];

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

struct Handle(*mut std::ffi::c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a live WinHTTP handle, closed once.
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn last_error(what: &str) -> String {
    format!("{what}: {}", std::io::Error::last_os_error())
}

/// Streams the body of `url` into `out`, at most `limit` bytes. `progress`
/// gets the bytes so far; raising `cancel` stops the transfer.
pub fn get(
    url: &str,
    accept: &str,
    limit: u64,
    out: &mut dyn Write,
    progress: &dyn Fn(u64),
    cancel: &AtomicBool,
) -> Result<u64> {
    let url_w = wide(url);
    let mut host = vec![0u16; 256];
    let mut path = vec![0u16; 2048];
    // SAFETY: plain-data struct; zero is a valid initial state.
    let mut parts: URL_COMPONENTS = unsafe { std::mem::zeroed() };
    parts.dwStructSize = std::mem::size_of::<URL_COMPONENTS>() as u32;
    parts.lpszHostName = host.as_mut_ptr();
    parts.dwHostNameLength = host.len() as u32;
    parts.lpszUrlPath = path.as_mut_ptr();
    parts.dwUrlPathLength = path.len() as u32;
    // SAFETY: the URL is NUL-terminated and the output buffers outlive the call.
    if unsafe { WinHttpCrackUrl(url_w.as_ptr(), 0, 0, &mut parts) } == 0 {
        return Err(last_error("bad update URL"));
    }
    let host_name = String::from_utf16_lossy(&host[..parts.dwHostNameLength as usize]);
    if parts.nScheme != WINHTTP_INTERNET_SCHEME_HTTPS
        || !HOSTS.iter().any(|h| h.eq_ignore_ascii_case(&host_name))
    {
        return Err(format!("update URL outside GitHub HTTPS: {url}"));
    }
    let agent = wide(&format!("AssetDoctorRTPT/{}", env!("CARGO_PKG_VERSION")));
    // SAFETY: NUL-terminated agent; null proxy strings mean the system default.
    let session = Handle(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(last_error("WinHTTP is unavailable"));
    }
    // SAFETY: live session handle; timeouts in milliseconds.
    unsafe { WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 60_000) };
    // SAFETY: live session; the host buffer is NUL-terminated by CrackUrl.
    let connection = Handle(unsafe { WinHttpConnect(session.0, host.as_ptr(), parts.nPort, 0) });
    if connection.0.is_null() {
        return Err(last_error("cannot reach GitHub"));
    }
    let verb = wide("GET");
    // SAFETY: live connection; NUL-terminated verb and path; no referrer or
    // explicit accept types.
    let request = Handle(unsafe {
        WinHttpOpenRequest(
            connection.0,
            verb.as_ptr(),
            path.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        )
    });
    if request.0.is_null() {
        return Err(last_error("cannot open the request"));
    }
    let headers = wide(&format!("Accept: {accept}\r\n"));
    // SAFETY: live request; the header string is NUL-terminated (length -1
    // = up to the NUL); no body.
    let sent = unsafe {
        WinHttpSendRequest(
            request.0,
            headers.as_ptr(),
            u32::MAX,
            std::ptr::null(),
            0,
            0,
            0,
        )
    };
    // SAFETY: live request after a successful send.
    if sent == 0 || unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) } == 0 {
        return Err(last_error("GitHub did not answer"));
    }
    let mut status = 0u32;
    let mut size = 4u32;
    // SAFETY: a numeric query into a u32 buffer of the given size.
    let queried = unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            std::ptr::null_mut(),
        )
    };
    if queried == 0 {
        return Err(last_error("no HTTP status"));
    }
    if status != 200 {
        return Err(match status {
            403 | 429 => "GitHub refused the request (rate limit?); try later".into(),
            404 => "not found on GitHub".into(),
            other => format!("GitHub answered HTTP {other}"),
        });
    }
    let mut buffer = vec![0u8; 256 << 10];
    let mut total = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let mut read = 0u32;
        // SAFETY: live request; the buffer is valid for its length.
        let ok = unsafe {
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
        };
        if ok == 0 {
            return Err(last_error("download interrupted"));
        }
        if read == 0 {
            break;
        }
        total += u64::from(read);
        if total > limit {
            return Err("download larger than expected".into());
        }
        out.write_all(&buffer[..read as usize])
            .map_err(|e| e.to_string())?;
        progress(total);
    }
    Ok(total)
}
