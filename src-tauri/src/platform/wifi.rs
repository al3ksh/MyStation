//! The Wi-Fi network the laptop is on and the networks it has saved, through the WLAN API.
//! Reading the SSID needs location access for desktop apps on Windows 11 24H2; without it the
//! network reads as none.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::WiFi::{
    WLAN_CONNECTION_ATTRIBUTES, WLAN_INTERFACE_INFO, WLAN_INTERFACE_INFO_LIST, WLAN_PROFILE_INFO,
    WLAN_PROFILE_INFO_LIST, WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory, WlanGetProfileList, WlanOpenHandle,
    WlanQueryInterface, wlan_interface_state_connected, wlan_intf_opcode_current_connection,
};

/// The WLAN client handle, closed on drop.
struct Client(HANDLE);

impl Client {
    fn open() -> Option<Self> {
        let (mut version, mut handle) = (0, HANDLE::default());
        // SAFETY: valid out-pointers; version 2 is the Vista+ API.
        let status = unsafe { WlanOpenHandle(2, None, &mut version, &mut handle) };
        (status == 0).then_some(Self(handle))
    }

    fn interfaces(&self) -> Vec<WLAN_INTERFACE_INFO> {
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        // SAFETY: the handle is open; the list is freed below.
        if unsafe { WlanEnumInterfaces(self.0, None, &mut list) } != 0 || list.is_null() {
            return Vec::new();
        }
        // SAFETY: the API returns `dwNumberOfItems` entries laid out after the header.
        let out = unsafe {
            let n = (*list).dwNumberOfItems as usize;
            std::slice::from_raw_parts((*list).InterfaceInfo.as_ptr(), n).to_vec()
        };
        // SAFETY: allocated by the WLAN API.
        unsafe { WlanFreeMemory(list as _) };
        out
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // SAFETY: opened by `WlanOpenHandle`.
        unsafe { WlanCloseHandle(self.0, None) };
    }
}

fn wide(s: &[u16]) -> String {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..end])
}

/// `Some(None)`: not on Wi-Fi. `None`: the WLAN service can't be reached, so nothing is known.
pub fn current_ssid() -> Option<Option<String>> {
    let client = Client::open()?;
    for i in client.interfaces().iter().filter(|i| i.isState == wlan_interface_state_connected) {
        let (mut size, mut data) = (0u32, std::ptr::null_mut());
        // SAFETY: valid out-pointers; the data is freed below.
        let status = unsafe {
            WlanQueryInterface(client.0, &i.InterfaceGuid, wlan_intf_opcode_current_connection, None, &mut size, &mut data, None)
        };
        if status != 0 || data.is_null() {
            continue;
        }
        // SAFETY: this opcode returns a `WLAN_CONNECTION_ATTRIBUTES`.
        let ssid = unsafe {
            let attrs = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
            let s = &attrs.wlanAssociationAttributes.dot11Ssid;
            let len = (s.uSSIDLength as usize).min(s.ucSSID.len());
            String::from_utf8_lossy(&s.ucSSID[..len]).into_owned()
        };
        // SAFETY: allocated by the WLAN API.
        unsafe { WlanFreeMemory(data as _) };
        if !ssid.is_empty() {
            return Some(Some(ssid));
        }
    }
    Some(None)
}

/// Saved network profiles, sorted. A profile is usually named after its network.
pub fn saved_networks() -> Vec<String> {
    let Some(client) = Client::open() else { return Vec::new() };
    let mut out = Vec::new();
    for i in client.interfaces() {
        let mut list: *mut WLAN_PROFILE_INFO_LIST = std::ptr::null_mut();
        // SAFETY: the handle is open; the list is freed below.
        if unsafe { WlanGetProfileList(client.0, &i.InterfaceGuid, None, &mut list) } != 0 || list.is_null() {
            continue;
        }
        // SAFETY: the API returns `dwNumberOfItems` entries laid out after the header.
        let profiles: &[WLAN_PROFILE_INFO] =
            unsafe { std::slice::from_raw_parts((*list).ProfileInfo.as_ptr(), (*list).dwNumberOfItems as usize) };
        out.extend(profiles.iter().map(|p| wide(&p.strProfileName)).filter(|n| !n.is_empty()));
        // SAFETY: allocated by the WLAN API.
        unsafe { WlanFreeMemory(list as _) };
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "reads this machine's Wi-Fi"]
    fn reads_the_network() {
        println!("{:?} {:?}", super::current_ssid(), super::saved_networks());
        assert!(super::current_ssid().is_some());
    }
}
