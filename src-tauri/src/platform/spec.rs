//! What the laptop has and supports, read from the device itself: CPUID, the SMBIOS memory
//! tables, the PnP manager's PCI, disk, Bluetooth and biometric devices, and the panel's EDID.
//! Nothing here is downloaded or changed.

use std::collections::HashMap;

use mystation_core::smbios::DeviceInfo;
use mystation_core::spec::{self, Cpu, Disk, Gpu, Spec};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_ALLCLASSES, DIGCF_PRESENT, GUID_DEVCLASS_BIOMETRIC, GUID_DEVCLASS_BLUETOOTH, GUID_DEVCLASS_DISKDRIVE,
    HDEVINFO, SETUP_DI_GET_CLASS_DEVS_FLAGS, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_CompatibleIds, DEVPKEY_Device_DeviceDesc, DEVPKEY_Device_Driver, DEVPKEY_Device_HardwareIds, DEVPKEY_Device_DriverVersion, DEVPKEY_Device_FriendlyName,
    DEVPKEY_Device_InstanceId, DEVPKEY_Device_Parent,
};
use windows::Win32::Foundation::DEVPROPKEY;
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformation, RelationCache, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
};
use windows::Win32::System::Threading::{IsProcessorFeaturePresent, PF_VIRT_FIRMWARE_ENABLED};
use windows::core::{GUID, PCWSTR, w};

use super::registry::{hklm_dword, hklm_qword};
use super::usb::{property, string};

/// The Bluetooth radio's LMP version, a byte.
const BT_LMP_VERSION: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0xa92f26ca_eda7_4b1d_9db2_27b68aa5a2eb), pid: 4 };

/// `DEVPKEY_PciDevice_*`, which the windows crate files under the Wi-Fi APIs.
const PCI: u128 = 0x3ab22e31_8264_4b4e_9af5_a8d2d8e33e62;
const PCI_BASE_CLASS: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(PCI), pid: 3 };
const PCI_MAX_LINK_SPEED: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(PCI), pid: 11 };
const PCI_MAX_LINK_WIDTH: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(PCI), pid: 12 };

/// PCI base classes.
const NETWORK: u32 = 2;
const DISPLAY: u32 = 3;

/// Calls `f` for every present device the set holds, then frees the set.
fn each(class: Option<&GUID>, enumerator: PCWSTR, mut f: impl FnMut(HDEVINFO, &SP_DEVINFO_DATA)) {
    let flags: SETUP_DI_GET_CLASS_DEVS_FLAGS =
        if class.is_some() { DIGCF_PRESENT } else { DIGCF_PRESENT | DIGCF_ALLCLASSES };
    // SAFETY: the device info set is destroyed before returning and only used while live.
    unsafe {
        let Ok(set) = SetupDiGetClassDevsW(class.map(|c| c as *const GUID), enumerator, None, flags) else {
            return;
        };
        let mut index = 0;
        loop {
            let mut info = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInfo(set, index, &mut info).is_err() {
                break;
            }
            index += 1;
            f(set, &info);
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
}

/// # Safety
/// `set` and `info` must be a live device info set and one of its elements.
unsafe fn number(set: HDEVINFO, info: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<u32> {
    let mut buf = [0u8; 4];
    unsafe { property(set, info, key, &mut buf) }.then(|| u32::from_le_bytes(buf))
}

/// # Safety
/// As for [`number`].
unsafe fn name(set: HDEVINFO, info: &SP_DEVINFO_DATA) -> Option<String> {
    unsafe { string(set, info, &DEVPKEY_Device_FriendlyName).or_else(|| string(set, info, &DEVPKEY_Device_DeviceDesc)) }
        .map(|n| n.trim().to_string())
}

struct PciDevice {
    id: String,
    name: String,
    base_class: Option<u32>,
    parent: Option<String>,
    driver_key: Option<String>,
    driver_version: Option<String>,
}

/// Every present PCI device, and the fastest link each one supports by instance ID.
fn pci() -> (Vec<PciDevice>, HashMap<String, (u32, u32)>) {
    let (mut devices, mut links) = (Vec::new(), HashMap::new());
    each(None, w!("PCI"), |set, info| {
        // SAFETY: `each` hands out live elements of a live set.
        unsafe {
            let Some(id) = string(set, info, &DEVPKEY_Device_InstanceId) else { return };
            if let (Some(speed), Some(width)) = (
                number(set, info, &PCI_MAX_LINK_SPEED),
                number(set, info, &PCI_MAX_LINK_WIDTH),
            ) {
                links.insert(id.to_ascii_uppercase(), (speed, width));
            }
            let Some(name) = name(set, info) else { return };
            devices.push(PciDevice {
                id,
                name,
                base_class: number(set, info, &PCI_BASE_CLASS).map(|c| c & 0xFF),
                parent: string(set, info, &DEVPKEY_Device_Parent),
                driver_key: string(set, info, &DEVPKEY_Device_Driver),
                driver_version: string(set, info, &DEVPKEY_Device_DriverVersion),
            });
        }
    });
    (devices, links)
}

/// The link of a PCIe device whose parent is a PCIe port; devices built into the chipset have none.
fn link_of(id: &str, parent: Option<&str>, links: &HashMap<String, (u32, u32)>) -> Option<spec::PciLink> {
    // Root ports often don't report their own maximum; then the device's is the link.
    let port = parent.and_then(|p| links.get(&p.to_ascii_uppercase()).copied());
    spec::pci_link(*links.get(&id.to_ascii_uppercase())?, port)
}

fn cpu_name() -> Option<String> {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::__cpuid;
        let mut bytes = Vec::with_capacity(48);
        for leaf in 0x8000_0002u32..=0x8000_0004 {
            // Every x86-64 CPU has the brand string leaves.
            let r = __cpuid(leaf);
            for reg in [r.eax, r.ebx, r.ecx, r.edx] {
                bytes.extend_from_slice(&reg.to_le_bytes());
            }
        }
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let name = String::from_utf8_lossy(&bytes[..end]).split_whitespace().collect::<Vec<_>>().join(" ");
        (!name.is_empty()).then_some(name)
    }
    #[cfg(not(target_arch = "x86_64"))]
    None
}

fn cpu_features() -> Vec<String> {
    let mut out = Vec::new();
    #[cfg(target_arch = "x86_64")]
    {
        let found = [
            (std::arch::is_x86_feature_detected!("avx2"), "AVX2"),
            (std::arch::is_x86_feature_detected!("avx512f"), "AVX-512"),
            (std::arch::is_x86_feature_detected!("aes"), "AES-NI"),
            (std::arch::is_x86_feature_detected!("sha"), "SHA"),
        ];
        out.extend(found.iter().filter(|(on, _)| *on).map(|(_, name)| (*name).to_string()));
    }
    out
}

/// Whether hardware virtualization is on: the firmware enabled it, or a hypervisor already runs.
fn virtualization() -> Option<bool> {
    // SAFETY: a plain query with no pointers.
    let firmware = unsafe { IsProcessorFeaturePresent(PF_VIRT_FIRMWARE_ENABLED) }.as_bool();
    #[cfg(target_arch = "x86_64")]
    // ECX bit 31 of leaf 1 is set under a hypervisor.
    let hypervisor = std::arch::x86_64::__cpuid(1).ecx & (1 << 31) != 0;
    #[cfg(not(target_arch = "x86_64"))]
    let hypervisor = false;
    Some(firmware || hypervisor)
}

fn cpu() -> Option<Cpu> {
    let mut len = 0u32;
    // SAFETY: a null buffer only asks for the size; the second call gets a buffer of that size.
    let entries = unsafe {
        let _ = GetLogicalProcessorInformation(None, &mut len);
        let count = len as usize / size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
        let mut buf = vec![SYSTEM_LOGICAL_PROCESSOR_INFORMATION::default(); count];
        GetLogicalProcessorInformation(Some(buf.as_mut_ptr()), &mut len).ok()?;
        buf.truncate(len as usize / size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>());
        buf
    };
    let mut cpu = Cpu { name: cpu_name()?, features: cpu_features(), virtualization: virtualization(), ..Cpu::default() };
    for e in &entries {
        if e.Relationship == RelationProcessorCore {
            cpu.cores += 1;
            cpu.threads += e.ProcessorMask.count_ones();
        } else if e.Relationship == RelationCache {
            // SAFETY: RelationCache entries hold the cache descriptor.
            let cache = unsafe { e.Anonymous.Cache };
            match cache.Level {
                2 => cpu.l2_kb += cache.Size / 1024,
                3 => cpu.l3_kb += cache.Size / 1024,
                _ => {}
            }
        }
    }
    Some(cpu)
}

fn graphics(devices: &[PciDevice], links: &HashMap<String, (u32, u32)>) -> Vec<Gpu> {
    devices
        .iter()
        .filter(|d| d.base_class == Some(DISPLAY))
        .map(|d| Gpu {
            name: d.name.clone(),
            driver: d.driver_version.clone(),
            vram_mb: d
                .driver_key
                .as_deref()
                .and_then(|k| hklm_qword(&format!(r"SYSTEM\CurrentControlSet\Control\Class\{k}"), "HardwareInformation.qwMemorySize"))
                .and_then(spec::vram_mb),
            link: link_of(&d.id, d.parent.as_deref(), links),
        })
        .collect()
}

/// The marketing size: decimal gigabytes, as on the box.
fn size_gb(bytes: u64) -> u64 {
    (bytes + 500_000_000) / 1_000_000_000
}

/// Internal disks with their capacity and the PCIe link of their NVMe controller.
fn storage(devices: &[PciDevice], links: &HashMap<String, (u32, u32)>) -> Vec<Disk> {
    let drives = super::storage::drives();
    let mut out = Vec::new();
    each(Some(&GUID_DEVCLASS_DISKDRIVE), PCWSTR::null(), |set, info| {
        // SAFETY: `each` hands out live elements of a live set.
        unsafe {
            let Some(id) = string(set, info, &DEVPKEY_Device_InstanceId) else { return };
            if id.to_ascii_uppercase().starts_with("USBSTOR") {
                return;
            }
            let Some(name) = name(set, info) else { return };
            let controller = string(set, info, &DEVPKEY_Device_Parent);
            let link = controller.as_deref().and_then(|c| {
                let pci = devices.iter().find(|d| d.id.eq_ignore_ascii_case(c))?;
                link_of(&pci.id, pci.parent.as_deref(), links)
            });
            let lower = name.to_ascii_lowercase();
            let size = drives
                .iter()
                .find(|d| {
                    d.identity.model.as_deref().map(str::trim).is_some_and(|m| {
                        let m = m.to_ascii_lowercase();
                        !m.is_empty() && (lower.contains(&m) || m.contains(&lower))
                    })
                })
                .or_else(|| (drives.len() == 1).then(|| &drives[0]))
                .and_then(|d| d.size_bytes);
            out.push(Disk { name, size_gb: size.map(size_gb), link });
        }
    });
    out
}

fn bluetooth() -> Option<String> {
    let mut version = None;
    each(Some(&GUID_DEVCLASS_BLUETOOTH), PCWSTR::null(), |set, info| {
        let mut lmp = [0u8; 1];
        // SAFETY: `each` hands out live elements of a live set.
        if version.is_none() && unsafe { property(set, info, &BT_LMP_VERSION, &mut lmp) } {
            version = spec::bluetooth_version(lmp[0]).map(str::to_string);
        }
    });
    version
}

fn fingerprint() -> Option<String> {
    let mut found = None;
    each(Some(&GUID_DEVCLASS_BIOMETRIC), PCWSTR::null(), |set, info| {
        // SAFETY: `each` hands out live elements of a live set.
        found = found.take().or_else(|| unsafe { name(set, info) });
    });
    found
}

/// Every ID in a hardware or compatible ID list, joined, upper case.
///
/// # Safety
/// As for [`property`].
unsafe fn ids(set: HDEVINFO, info: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> String {
    let mut buf = [0u8; 2048];
    if !unsafe { property(set, info, key, &mut buf) } {
        return String::new();
    }
    let wide: Vec<u16> = buf.as_chunks::<2>().0.iter().map(|&c| u16::from_le_bytes(c)).collect();
    String::from_utf16_lossy(&wide).to_ascii_uppercase()
}

/// The TPM's version from its ACPI IDs: MSFT0101 is TPM 2.0, PNP0C31 TPM 1.2. Firmware TPMs
/// carry the vendor's own ID and list MSFT0101 among the compatible ones.
fn tpm() -> Option<String> {
    let mut found = None;
    each(None, w!("ACPI"), |set, info| {
        // SAFETY: `each` hands out live elements of a live set.
        let id = unsafe { ids(set, info, &DEVPKEY_Device_HardwareIds) + &ids(set, info, &DEVPKEY_Device_CompatibleIds) };
        if id.contains("MSFT0101") {
            found = Some("2.0");
        } else if id.contains("PNP0C31") && found.is_none() {
            found = Some("1.2");
        }
    });
    found.map(str::to_string)
}

/// The first network controller whose name says it's Wi-Fi.
fn wifi(devices: &[PciDevice]) -> Option<String> {
    let words = ["wi-fi", "wifi", "wireless", "wlan", "802.11"];
    devices
        .iter()
        .filter(|d| d.base_class == Some(NETWORK))
        .find(|d| words.iter().any(|w| d.name.to_ascii_lowercase().contains(w)))
        .map(|d| d.name.clone())
}

fn thunderbolt(devices: &[PciDevice]) -> Option<String> {
    let names = devices.iter().map(|d| d.name.to_ascii_lowercase());
    let mut found = None;
    for n in names {
        if n.contains("usb4") {
            return Some("USB4".to_string());
        }
        if n.contains("thunderbolt") {
            found = Some("Thunderbolt".to_string());
        }
    }
    found
}

/// Everything the laptop reports about itself; takes a few hundred milliseconds.
pub fn spec(info: &DeviceInfo) -> Spec {
    let (devices, links) = pci();
    Spec {
        model: info.model.clone(),
        bios_version: info.bios_version.clone(),
        bios_date: info.bios_date.clone(),
        cpu: cpu(),
        memory: super::smbios::memory(),
        graphics: graphics(&devices, &links),
        storage: storage(&devices, &links),
        panel: spec::panel(&super::display::displays()),
        wifi: wifi(&devices),
        bluetooth: bluetooth(),
        thunderbolt: thunderbolt(&devices),
        tpm: tpm(),
        secure_boot: hklm_dword(r"SYSTEM\CurrentControlSet\Control\SecureBoot\State", "UEFISecureBootEnabled").map(|v| v != 0),
        fingerprint: fingerprint(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn marketing_sizes() {
        assert_eq!(super::size_gb(512_110_190_592), 512);
        assert_eq!(super::size_gb(1_024_209_543_168), 1024);
    }

    /// Hardware probe: `cargo test -p mystation prints_the_spec -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn prints_the_spec() {
        println!("{:#?}", super::spec(&super::super::device_info()));
    }
}
