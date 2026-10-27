//! Physical drives: identity and the NVMe SMART / Health Information log via
//! `IOCTL_STORAGE_QUERY_PROPERTY`. The health log needs an elevated process.

use mystation_core::nvme::{self, DriveIdentity, SmartLog};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW, OPEN_EXISTING,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{DISK_GEOMETRY_EX, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, IOCTL_STORAGE_QUERY_PROPERTY};
use windows::core::{HSTRING, w};

pub struct Drive {
    pub index: u32,
    pub identity: DriveIdentity,
    pub smart: Option<SmartLog>,
    pub size_bytes: Option<u64>,
}

/// Physical drives probed; laptops have one or two.
const MAX_DRIVES: u32 = 8;

pub fn drives() -> Vec<Drive> {
    (0..MAX_DRIVES).filter_map(read_drive).collect()
}

fn read_drive(index: u32) -> Option<Drive> {
    let handle = open(index)?;
    let identity = query_descriptor(handle).map(|d| nvme::parse_device_descriptor(&d)).unwrap_or_default();
    let smart = if identity.nvme { query_smart_log(handle).and_then(|l| nvme::parse_smart_log(&l)) } else { None };
    let size_bytes = disk_size(handle);
    // SAFETY: `handle` came from CreateFileW and is closed once.
    unsafe {
        let _ = CloseHandle(handle);
    }
    Some(Drive { index, identity, smart, size_bytes })
}

fn open(index: u32) -> Option<HANDLE> {
    let path = HSTRING::from(format!(r"\\.\PhysicalDrive{index}"));
    // Read/write access is what protocol-specific queries require; it opens the device
    // for IOCTLs only, nothing here writes to the disk. Without elevation, fall back
    // to no access, which still answers the descriptor query.
    [GENERIC_READ.0 | GENERIC_WRITE.0, 0].into_iter().find_map(|access| {
        // SAFETY: `path` outlives the call; no security attributes or template.
        unsafe {
            CreateFileW(
                &path,
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .ok()
    })
}

fn ioctl(handle: HANDLE, code: u32, input: &[u8], out_len: usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; out_len];
    let mut returned = 0u32;
    // SAFETY: both buffers are live for the call and their lengths are passed alongside.
    unsafe {
        DeviceIoControl(
            handle,
            code,
            Some(input.as_ptr().cast()),
            input.len() as u32,
            Some(out.as_mut_ptr().cast()),
            out.len() as u32,
            Some(&mut returned),
            None,
        )
    }
    .ok()?;
    out.truncate(returned as usize);
    Some(out)
}

/// `STORAGE_PROPERTY_QUERY { PropertyId: StorageDeviceProperty, QueryType: PropertyStandardQuery }`.
fn query_descriptor(handle: HANDLE) -> Option<Vec<u8>> {
    ioctl(handle, IOCTL_STORAGE_QUERY_PROPERTY, &[0u8; 12], 1024)
}

/// The disk's capacity in bytes; answers with no access rights.
fn disk_size(handle: HANDLE) -> Option<u64> {
    let out = ioctl(handle, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, &[], size_of::<DISK_GEOMETRY_EX>())?;
    let at = std::mem::offset_of!(DISK_GEOMETRY_EX, DiskSize);
    out.get(at..at + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap())).filter(|&n| n > 0)
}

const LOG_PAGE_SMART: u32 = 0x02;
const LOG_LEN: usize = 512;
/// `sizeof(STORAGE_PROTOCOL_SPECIFIC_DATA)`.
const PROTOCOL_DATA_LEN: usize = 40;

fn query_smart_log(handle: HANDLE) -> Option<Vec<u8>> {
    // STORAGE_PROPERTY_QUERY header, then STORAGE_PROTOCOL_SPECIFIC_DATA in AdditionalParameters.
    let mut query = vec![0u8; 8 + PROTOCOL_DATA_LEN + LOG_LEN];
    let mut put = |at: usize, v: u32| query[at..at + 4].copy_from_slice(&v.to_le_bytes());
    put(0, 50); // StorageDeviceProtocolSpecificProperty
    put(4, 0); // PropertyStandardQuery
    put(8, 3); // ProtocolTypeNvme
    put(12, 2); // NVMeDataTypeLogPage
    put(16, LOG_PAGE_SMART);
    put(24, PROTOCOL_DATA_LEN as u32); // ProtocolDataOffset
    put(28, LOG_LEN as u32); // ProtocolDataLength

    // STORAGE_PROTOCOL_DATA_DESCRIPTOR: Version, Size, then the protocol data header.
    let out = ioctl(handle, IOCTL_STORAGE_QUERY_PROPERTY, &query, query.len())?;
    let offset = u32::from_le_bytes(out.get(24..28)?.try_into().ok()?) as usize;
    let len = u32::from_le_bytes(out.get(28..32)?.try_into().ok()?) as usize;
    out.get(8 + offset..8 + offset + len.min(LOG_LEN)).map(<[u8]>::to_vec)
}

/// Total and free bytes of the Windows volume.
pub fn system_volume() -> Option<(u64, u64)> {
    let (mut free, mut total) = (0u64, 0u64);
    // SAFETY: the out pointers are live locals.
    unsafe { GetDiskFreeSpaceExW(w!(r"C:\"), Some(&mut free), Some(&mut total), None) }.ok()?;
    Some((total, free))
}

#[cfg(test)]
mod tests {
    /// Hardware probe (elevated for the health log): `cargo test -p mystation drives -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn prints_drives() {
        for d in super::drives() {
            println!("{} {:?}\n{:#?}", d.index, d.identity, d.smart);
        }
        println!("{:?}", super::system_volume());
    }
}
