//! Present USB devices from the PnP manager: name, last arrival, removability and power state,
//! plus the link speed their hub reports for the port they're in.

use std::collections::HashMap;

use mystation_core::usb::{RawUsbDevice, UsbLink, UsbSpeed};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_DEVCAP_REMOVABLE, CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
    CM_Get_Device_Interface_ListW, CR_SUCCESS, DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SP_DEVINFO_DATA,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDevicePropertyW,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_Address, DEVPKEY_Device_BusReportedDeviceDesc, DEVPKEY_Device_Capabilities,
    DEVPKEY_Device_DeviceDesc, DEVPKEY_Device_FriendlyName, DEVPKEY_Device_InstanceId, DEVPKEY_Device_LastArrivalDate,
    DEVPKEY_Device_Parent, DEVPKEY_Device_PowerData, DEVPROPTYPE,
};
use windows::Win32::Devices::Usb::{
    GUID_DEVINTERFACE_USB_HUB, IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX,
    IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX_V2, IOCTL_USB_GET_PORT_CONNECTOR_PROPERTIES,
    USB_NODE_CONNECTION_INFORMATION_EX, USB_NODE_CONNECTION_INFORMATION_EX_V2, USB_PORT_CONNECTOR_PROPERTIES,
};
use windows::Win32::Foundation::{CloseHandle, DEVPROPKEY, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_WRITE, OPEN_EXISTING};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Power::{CM_POWER_DATA, PowerDeviceD0};
use windows::core::{HSTRING, PCWSTR, w};

use super::gpu_power::utf16_string;

/// FILETIME of the Unix epoch, in 100 ns units.
const UNIX_EPOCH_FILETIME: u64 = 116_444_736_000_000_000;

/// `USB_PROTOCOLS.Usb300`
const PROTOCOL_USB3: u32 = 0x4;
/// `USB_NODE_CONNECTION_INFORMATION_EX_V2_FLAGS` bits.
const SUPERSPEED_CAPABLE: u32 = 0x2;
const OPERATING_AT_SUPERSPEED_PLUS: u32 = 0x4;

/// Reads one property into `buf`; `false` when the device doesn't have it.
///
/// # Safety
/// `set` and `info` must be a live device info set and one of its elements.
pub(super) unsafe fn property(set: HDEVINFO, info: &SP_DEVINFO_DATA, key: &DEVPROPKEY, buf: &mut [u8]) -> bool {
    let mut ty = DEVPROPTYPE::default();
    unsafe { SetupDiGetDevicePropertyW(set, info, key, &mut ty, Some(buf), None, 0) }.is_ok()
}

/// # Safety
/// As for [`property`].
pub(super) unsafe fn string(set: HDEVINFO, info: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<String> {
    let mut buf = [0u8; 1024];
    unsafe { property(set, info, key, &mut buf) }.then(|| utf16_string(&buf)).filter(|s| !s.trim().is_empty())
}

/// Open handles to the hubs devices hang off, by hub instance ID, closed on drop.
#[derive(Default)]
struct Hubs(HashMap<String, Option<HANDLE>>);

impl Hubs {
    fn get(&mut self, instance_id: &str) -> Option<HANDLE> {
        *self.0.entry(instance_id.to_ascii_uppercase()).or_insert_with(|| open_hub(instance_id))
    }
}

impl Drop for Hubs {
    fn drop(&mut self) {
        for handle in self.0.values().flatten() {
            // SAFETY: each handle came from CreateFileW and is closed once.
            unsafe {
                let _ = CloseHandle(*handle);
            }
        }
    }
}

/// The hub's device interface, opened for IOCTLs. Hub queries don't need elevation.
fn open_hub(instance_id: &str) -> Option<HANDLE> {
    let id = HSTRING::from(instance_id);
    let flags = CM_GET_DEVICE_INTERFACE_LIST_PRESENT;
    // SAFETY: `id` and the buffer outlive the calls; the buffer is sized by the first call.
    unsafe {
        let mut len = 0u32;
        if CM_Get_Device_Interface_List_SizeW(&mut len, &GUID_DEVINTERFACE_USB_HUB, &id, flags) != CR_SUCCESS
            || len < 2
        {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        if CM_Get_Device_Interface_ListW(&GUID_DEVINTERFACE_USB_HUB, &id, &mut buf, flags) != CR_SUCCESS {
            return None;
        }
        CreateFileW(
            PCWSTR(buf.as_ptr()),
            GENERIC_WRITE.0,
            FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
        .ok()
    }
}

/// Sends `data` to the hub and reads the answer back into it.
///
/// # Safety
/// `T` must be the plain-data struct `code` takes as input and returns as output.
unsafe fn hub_ioctl<T: Copy>(hub: HANDLE, code: u32, mut data: T) -> Option<T> {
    let len = size_of::<T>() as u32;
    let ptr = (&raw mut data).cast();
    // SAFETY: `data` is live for the call and its size is passed alongside.
    unsafe { DeviceIoControl(hub, code, Some(ptr), len, Some(ptr), len, None, None) }.ok()?;
    Some(data)
}

/// The speed of the device in `port`, and what the device and port could do.
fn link(hub: HANDLE, port: u32) -> Option<UsbLink> {
    // SAFETY: each struct is the one its IOCTL is documented to take and return.
    unsafe {
        // The trailing pipe list is variable-length; the hub fills what fits and still succeeds.
        let ex = hub_ioctl(
            hub,
            IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX,
            USB_NODE_CONNECTION_INFORMATION_EX { ConnectionIndex: port, ..Default::default() },
        )?;
        let mut v2 = USB_NODE_CONNECTION_INFORMATION_EX_V2 {
            ConnectionIndex: port,
            Length: size_of::<USB_NODE_CONNECTION_INFORMATION_EX_V2>() as u32,
            ..Default::default()
        };
        // Offer every protocol; the hub answers with the ones the port supports.
        v2.SupportedUsbProtocols.ul = 0xF;
        let v2 = hub_ioctl(hub, IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX_V2, v2);
        let flags = v2.map_or(0, |v| v.Flags.ul);
        let protocols = v2.map_or(0, |v| v.SupportedUsbProtocols.ul);
        // A USB 2 port with a USB 3 twin: the connector has a SuperSpeed lane the device didn't get.
        let connector = hub_ioctl(
            hub,
            IOCTL_USB_GET_PORT_CONNECTOR_PROPERTIES,
            USB_PORT_CONNECTOR_PROPERTIES { ConnectionIndex: port, ..Default::default() },
        );
        let companion = connector.is_some_and(|c| c.CompanionPortNumber != 0);

        let speed = match ex.Speed {
            0 => UsbSpeed::Low,
            1 => UsbSpeed::Full,
            2 => UsbSpeed::High,
            _ if flags & OPERATING_AT_SUPERSPEED_PLUS != 0 => UsbSpeed::SuperPlus,
            _ => UsbSpeed::Super,
        };
        Some(UsbLink {
            speed,
            superspeed_capable: flags & SUPERSPEED_CAPABLE != 0,
            superspeed_port: protocols & PROTOCOL_USB3 != 0 || companion,
        })
    }
}

/// # Safety
/// As for [`property`].
unsafe fn device(set: HDEVINFO, info: &SP_DEVINFO_DATA, hubs: &mut Hubs) -> Option<RawUsbDevice> {
    unsafe {
        let id = string(set, info, &DEVPKEY_Device_InstanceId)?;
        let upper = id.to_ascii_uppercase();
        // Root hubs belong to the controller; `&MI_` entries are interfaces of a composite device.
        if upper.starts_with(r"USB\ROOT_HUB") || upper.contains("&MI_") {
            return None;
        }
        let name = [DEVPKEY_Device_BusReportedDeviceDesc, DEVPKEY_Device_FriendlyName, DEVPKEY_Device_DeviceDesc]
            .iter()
            .find_map(|key| string(set, info, key))?;

        let mut filetime = [0u8; 8];
        let arrived_ms = property(set, info, &DEVPKEY_Device_LastArrivalDate, &mut filetime)
            .then(|| u64::from_le_bytes(filetime))
            .filter(|ft| *ft > UNIX_EPOCH_FILETIME)
            .map(|ft| ((ft - UNIX_EPOCH_FILETIME) / 10_000) as i64);

        let mut caps = [0u8; 4];
        let removable = property(set, info, &DEVPKEY_Device_Capabilities, &mut caps)
            && u32::from_le_bytes(caps) & CM_DEVCAP_REMOVABLE.0 != 0;

        let mut data = CM_POWER_DATA::default();
        let buf = std::slice::from_raw_parts_mut((&raw mut data).cast::<u8>(), size_of::<CM_POWER_DATA>());
        let suspended =
            property(set, info, &DEVPKEY_Device_PowerData, buf) && data.PD_MostRecentPowerState != PowerDeviceD0;

        // The port number on the parent hub.
        let mut address = [0u8; 4];
        let link = string(set, info, &DEVPKEY_Device_Parent)
            .filter(|_| property(set, info, &DEVPKEY_Device_Address, &mut address))
            .and_then(|parent| hubs.get(&parent))
            .and_then(|hub| link(hub, u32::from_le_bytes(address)));

        Some(RawUsbDevice { id, name: name.trim().to_string(), arrived_ms, removable, suspended, link })
    }
}

/// Every present device the USB bus enumerated.
pub fn devices() -> Vec<RawUsbDevice> {
    let mut out = Vec::new();
    let mut hubs = Hubs::default();
    // SAFETY: the device info set is destroyed before returning and only used while live.
    unsafe {
        let Ok(set) = SetupDiGetClassDevsW(None, w!("USB"), None, DIGCF_PRESENT | DIGCF_ALLCLASSES) else {
            return out;
        };
        let mut index = 0;
        loop {
            let mut info = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInfo(set, index, &mut info).is_err() {
                break;
            }
            index += 1;
            out.extend(device(set, &info, &mut hubs));
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    out
}

