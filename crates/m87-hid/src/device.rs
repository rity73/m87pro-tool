//! Safe wrapper around one opened HID interface, plus enumeration.

use std::ffi::c_void;
use std::io;
use std::ptr::null_mut;

use crate::ffi::*;

/// Information about one HID interface, gathered without keeping it open.
#[derive(Debug, Clone)]
pub struct HidInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// USB interface number parsed from the path (`mi_03` → 3), or -1.
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub input_len: u16,
    pub output_len: u16,
    pub feature_len: u16,
    pub product: String,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn interface_from_path(path: &str) -> i32 {
    let lower = path.to_ascii_lowercase();
    lower
        .find("&mi_")
        .and_then(|i| lower.get(i + 4..i + 6))
        .and_then(|h| i32::from_str_radix(h, 16).ok())
        .unwrap_or(-1)
}

/// Owned file handle that closes on drop.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn open_raw(path: &str, access: u32) -> io::Result<Handle> {
    let w = wide(path);
    let h = unsafe {
        CreateFileW(
            w.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(Handle(h))
    }
}

fn query(path: String) -> Option<HidInfo> {
    // Access 0 is enough for attributes and caps, and works even for
    // keyboard/mouse collections that Windows keeps opened exclusively.
    let h = open_raw(&path, 0).ok()?;
    let mut attrs = HIDD_ATTRIBUTES { Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
    if unsafe { HidD_GetAttributes(h.0, &mut attrs) } == 0 {
        return None;
    }
    let mut caps = HIDP_CAPS::default();
    let mut pp: isize = 0;
    if unsafe { HidD_GetPreparsedData(h.0, &mut pp) } != 0 {
        unsafe {
            if HidP_GetCaps(pp, &mut caps) != HIDP_STATUS_SUCCESS {
                caps = HIDP_CAPS::default();
            }
            HidD_FreePreparsedData(pp);
        }
    }
    let mut buf = [0u16; 128];
    let product = if unsafe { HidD_GetProductString(h.0, buf.as_mut_ptr() as *mut c_void, 256) } != 0 {
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    } else {
        String::new()
    };
    Some(HidInfo {
        interface: interface_from_path(&path),
        path,
        vendor_id: attrs.VendorID,
        product_id: attrs.ProductID,
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        input_len: caps.InputReportByteLength,
        output_len: caps.OutputReportByteLength,
        feature_len: caps.FeatureReportByteLength,
        product,
    })
}

/// Enumerate every present HID interface whose vendor ID matches.
pub fn enumerate(vendor_id: u16) -> Vec<HidInfo> {
    let mut out = Vec::new();
    let mut guid = GUID::default();
    unsafe { HidD_GetHidGuid(&mut guid) };
    let set = unsafe { SetupDiGetClassDevsW(&guid, std::ptr::null(), null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE) };
    if set == INVALID_HANDLE_VALUE {
        return out;
    }
    let mut index = 0;
    loop {
        let mut data = SP_DEVICE_INTERFACE_DATA {
            cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            InterfaceClassGuid: GUID::default(),
            Flags: 0,
            Reserved: 0,
        };
        if unsafe { SetupDiEnumDeviceInterfaces(set, null_mut(), &guid, index, &mut data) } == 0 {
            break;
        }
        index += 1;
        let mut required = 0u32;
        unsafe { SetupDiGetDeviceInterfaceDetailW(set, &mut data, null_mut(), 0, &mut required, null_mut()) };
        if required < 8 {
            continue;
        }
        // u32-aligned buffer; first field is cbSize (8 on 64-bit, 6 on 32-bit).
        let mut buf = vec![0u32; (required as usize).div_ceil(4)];
        buf[0] = if cfg!(target_pointer_width = "64") { 8 } else { 6 };
        let ok = unsafe {
            SetupDiGetDeviceInterfaceDetailW(set, &mut data, buf.as_mut_ptr() as *mut c_void, required, null_mut(), null_mut())
        };
        if ok == 0 {
            continue;
        }
        let words: &[u16] = unsafe { std::slice::from_raw_parts((buf.as_ptr() as *const u16).add(2), (required as usize - 4) / 2) };
        let end = words.iter().position(|c| *c == 0).unwrap_or(words.len());
        let path = String::from_utf16_lossy(&words[..end]);
        if !path.to_ascii_lowercase().contains(&format!("vid_{vendor_id:04x}")) {
            continue;
        }
        if let Some(info) = query(path) {
            if info.vendor_id == vendor_id {
                out.push(info);
            }
        }
    }
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    out
}

/// An opened HID interface with overlapped (timeout-capable) I/O.
pub struct HidDevice {
    handle: Handle,
    event: Handle,
    pub info: HidInfo,
}

impl HidDevice {
    pub fn open(info: &HidInfo) -> io::Result<Self> {
        let handle = open_raw(&info.path, GENERIC_READ | GENERIC_WRITE)?;
        let event = unsafe { CreateEventW(null_mut(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(HidDevice { handle, event: Handle(event), info: info.clone() })
    }

    fn overlapped(&self) -> OVERLAPPED {
        unsafe { ResetEvent(self.event.0) };
        OVERLAPPED { Internal: 0, InternalHigh: 0, Offset: 0, OffsetHigh: 0, hEvent: self.event.0 }
    }

    /// Write one output report (the first byte is the report ID).
    pub fn write(&self, data: &[u8]) -> io::Result<usize> {
        let mut ov = self.overlapped();
        let mut n = 0u32;
        let ok = unsafe { WriteFile(self.handle.0, data.as_ptr() as *const c_void, data.len() as u32, null_mut(), &mut ov) };
        if ok == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
            return Err(io::Error::last_os_error());
        }
        if unsafe { GetOverlappedResult(self.handle.0, &mut ov, &mut n, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n as usize)
    }

    /// Read one input report, waiting at most `timeout_ms`. Returns 0 on
    /// timeout. The report ID byte is kept (buf[0]).
    pub fn read_timeout(&self, buf: &mut [u8], timeout_ms: u32) -> io::Result<usize> {
        let mut ov = self.overlapped();
        let mut n = 0u32;
        let ok = unsafe { ReadFile(self.handle.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, null_mut(), &mut ov) };
        if ok == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
            return Err(io::Error::last_os_error());
        }
        if unsafe { WaitForSingleObject(self.event.0, timeout_ms) } != WAIT_OBJECT_0 {
            unsafe {
                CancelIo(self.handle.0);
                // Wait for the cancel to finish so `buf` is no longer in use.
                GetOverlappedResult(self.handle.0, &mut ov, &mut n, 1);
            }
            return Ok(0);
        }
        if unsafe { GetOverlappedResult(self.handle.0, &mut ov, &mut n, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n as usize)
    }

    /// Send a feature report (first byte is the report ID).
    pub fn set_feature(&self, data: &[u8]) -> io::Result<()> {
        let mut copy = data.to_vec();
        if unsafe { HidD_SetFeature(self.handle.0, copy.as_mut_ptr() as *mut c_void, copy.len() as u32) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Read a feature report into `buf` (buf[0] must hold the report ID).
    pub fn get_feature(&self, buf: &mut [u8]) -> io::Result<()> {
        if unsafe { HidD_GetFeature(self.handle.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mi_parsing() {
        assert_eq!(interface_from_path(r"\\?\hid#vid_0c45&pid_fefe&mi_03#7&200725c&0&0000#{4d1e55b2}"), 3);
        assert_eq!(interface_from_path(r"\\?\hid#vid_0c45&pid_fefe&mi_02&col05#x"), 2);
        assert_eq!(interface_from_path(r"\\?\hid#vid_0c45&pid_8006#x"), -1);
    }
}
