//! Minimal Win32 bindings (hid.dll, setupapi.dll, kernel32.dll).
//!
//! Hand-written on purpose: crates such as `windows-sys` or `hidapi` run build
//! scripts, and Smart App Control on the development PC blocks unsigned build
//! scripts. These declarations need only the import libraries from the
//! Windows SDK.
#![allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms)]

use std::ffi::c_void;

pub type HANDLE = *mut c_void;
pub type BOOL = i32;
pub type BOOLEAN = u8;
pub type HDEVINFO = *mut c_void;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const GENERIC_READ: u32 = 0x8000_0000;
pub const GENERIC_WRITE: u32 = 0x4000_0000;
pub const FILE_SHARE_READ: u32 = 0x1;
pub const FILE_SHARE_WRITE: u32 = 0x2;
pub const OPEN_EXISTING: u32 = 3;
pub const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
pub const DIGCF_PRESENT: u32 = 0x2;
pub const DIGCF_DEVICEINTERFACE: u32 = 0x10;
pub const ERROR_IO_PENDING: u32 = 997;
pub const WAIT_OBJECT_0: u32 = 0;
pub const HIDP_STATUS_SUCCESS: i32 = 0x0011_0000;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[repr(C)]
pub struct SP_DEVICE_INTERFACE_DATA {
    pub cbSize: u32,
    pub InterfaceClassGuid: GUID,
    pub Flags: u32,
    pub Reserved: usize,
}

#[repr(C)]
#[derive(Default)]
pub struct HIDD_ATTRIBUTES {
    pub Size: u32,
    pub VendorID: u16,
    pub ProductID: u16,
    pub VersionNumber: u16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct HIDP_CAPS {
    pub Usage: u16,
    pub UsagePage: u16,
    pub InputReportByteLength: u16,
    pub OutputReportByteLength: u16,
    pub FeatureReportByteLength: u16,
    pub Reserved: [u16; 17],
    pub NumberLinkCollectionNodes: u16,
    pub NumberInputButtonCaps: u16,
    pub NumberInputValueCaps: u16,
    pub NumberInputDataIndices: u16,
    pub NumberOutputButtonCaps: u16,
    pub NumberOutputValueCaps: u16,
    pub NumberOutputDataIndices: u16,
    pub NumberFeatureButtonCaps: u16,
    pub NumberFeatureValueCaps: u16,
    pub NumberFeatureDataIndices: u16,
}

#[repr(C)]
pub struct OVERLAPPED {
    pub Internal: usize,
    pub InternalHigh: usize,
    pub Offset: u32,
    pub OffsetHigh: u32,
    pub hEvent: HANDLE,
}

#[link(name = "hid")]
extern "system" {
    pub fn HidD_GetHidGuid(guid: *mut GUID);
    pub fn HidD_GetAttributes(dev: HANDLE, attrs: *mut HIDD_ATTRIBUTES) -> BOOLEAN;
    pub fn HidD_GetPreparsedData(dev: HANDLE, data: *mut isize) -> BOOLEAN;
    pub fn HidD_FreePreparsedData(data: isize) -> BOOLEAN;
    pub fn HidP_GetCaps(data: isize, caps: *mut HIDP_CAPS) -> i32;
    pub fn HidD_GetProductString(dev: HANDLE, buf: *mut c_void, len: u32) -> BOOLEAN;
    pub fn HidD_SetFeature(dev: HANDLE, buf: *mut c_void, len: u32) -> BOOLEAN;
    pub fn HidD_GetFeature(dev: HANDLE, buf: *mut c_void, len: u32) -> BOOLEAN;
}

#[link(name = "setupapi")]
extern "system" {
    pub fn SetupDiGetClassDevsW(guid: *const GUID, enumerator: *const u16, parent: HANDLE, flags: u32) -> HDEVINFO;
    pub fn SetupDiEnumDeviceInterfaces(
        set: HDEVINFO,
        info: *mut c_void,
        guid: *const GUID,
        index: u32,
        data: *mut SP_DEVICE_INTERFACE_DATA,
    ) -> BOOL;
    pub fn SetupDiGetDeviceInterfaceDetailW(
        set: HDEVINFO,
        data: *mut SP_DEVICE_INTERFACE_DATA,
        detail: *mut c_void,
        detail_size: u32,
        required: *mut u32,
        info: *mut c_void,
    ) -> BOOL;
    pub fn SetupDiDestroyDeviceInfoList(set: HDEVINFO) -> BOOL;
}

#[link(name = "kernel32")]
extern "system" {
    pub fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: HANDLE,
    ) -> HANDLE;
    pub fn CloseHandle(h: HANDLE) -> BOOL;
    pub fn WriteFile(h: HANDLE, buf: *const c_void, len: u32, written: *mut u32, ov: *mut OVERLAPPED) -> BOOL;
    pub fn ReadFile(h: HANDLE, buf: *mut c_void, len: u32, read: *mut u32, ov: *mut OVERLAPPED) -> BOOL;
    pub fn GetOverlappedResult(h: HANDLE, ov: *mut OVERLAPPED, n: *mut u32, wait: BOOL) -> BOOL;
    pub fn CancelIo(h: HANDLE) -> BOOL;
    pub fn CreateEventW(attrs: *mut c_void, manual_reset: BOOL, initial: BOOL, name: *const u16) -> HANDLE;
    pub fn ResetEvent(h: HANDLE) -> BOOL;
    pub fn WaitForSingleObject(h: HANDLE, ms: u32) -> u32;
    pub fn GetLastError() -> u32;
}
