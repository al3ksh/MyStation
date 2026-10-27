//! Paired Bluetooth devices from the PnP manager: container, category, connection and battery.

use mystation_core::bluetooth::RawBtNode;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_ALLCLASSES, DIGCF_PRESENT, HDEVINFO, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_ContainerId, DEVPKEY_Device_FriendlyName, DEVPKEY_Device_InstanceId,
    DEVPKEY_DeviceContainer_Category,
};
use windows::Win32::Foundation::DEVPROPKEY;
use windows::core::{GUID, PCWSTR, w};

use super::usb::{property, string};

/// The battery level the Bluetooth stack reads from the device's Battery Service or HFP, a byte.
const BATTERY_PCT: DEVPROPKEY =
    DEVPROPKEY { fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5), pid: 2 };
/// Whether the device is connected right now, a boolean.
const CONNECTED: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0x83da6326_97a6_4088_9453_a1923f573b29), pid: 15 };

/// # Safety
/// `set` and `info` must be a live device info set and one of its elements.
unsafe fn node(set: HDEVINFO, info: &SP_DEVINFO_DATA) -> Option<RawBtNode> {
    unsafe {
        let id = string(set, info, &DEVPKEY_Device_InstanceId)?;
        // Profiles and services hang off the device as `BTHENUM\{uuid}…` nodes.
        if !id.to_ascii_uppercase().split('\\').nth(1).is_some_and(|s| s.starts_with("DEV_")) {
            return None;
        }
        let mut guid = GUID::zeroed();
        let buf = std::slice::from_raw_parts_mut((&raw mut guid).cast::<u8>(), size_of::<GUID>());
        let container = property(set, info, &DEVPKEY_Device_ContainerId, buf).then(|| format!("{guid:?}"))?;
        let mut flag = [0u8; 1];
        let connected = property(set, info, &CONNECTED, &mut flag) && flag[0] != 0;
        let mut pct = [0u8; 1];
        let battery_pct = property(set, info, &BATTERY_PCT, &mut pct).then_some(pct[0]);
        Some(RawBtNode {
            container,
            name: string(set, info, &DEVPKEY_Device_FriendlyName)?,
            category: string(set, info, &DEVPKEY_DeviceContainer_Category),
            connected,
            battery_pct,
        })
    }
}

fn enumerate(enumerator: PCWSTR, out: &mut Vec<RawBtNode>) {
    // SAFETY: the device info set is destroyed before returning and only used while live.
    unsafe {
        let Ok(set) = SetupDiGetClassDevsW(None, enumerator, None, DIGCF_PRESENT | DIGCF_ALLCLASSES) else {
            return;
        };
        let mut index = 0;
        loop {
            let mut info = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInfo(set, index, &mut info).is_err() {
                break;
            }
            index += 1;
            out.extend(node(set, &info));
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
}

/// Every paired Bluetooth device node, Classic and LE.
pub fn nodes() -> Vec<RawBtNode> {
    let mut out = Vec::new();
    enumerate(w!("BTHENUM"), &mut out);
    enumerate(w!("BTHLE"), &mut out);
    out
}

