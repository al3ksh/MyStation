//! What the laptop has and supports, from what the device reports about itself.

use serde::Serialize;

use crate::display::{Connection, Mode, RawDisplay, parse_edid};
use crate::smbios::Memory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PciLink {
    /// PCIe generation: 3 for PCIe 3.0.
    pub generation: u8,
    pub lanes: u8,
}

/// The link a PCIe device can reach: the slower of what it and its port support. The current
/// link isn't it, as devices drop speed and lanes when idle. Speeds are the `MaxLinkSpeed`
/// codes, 1 for 2.5 GT/s (PCIe 1) up, paired with the lane count.
pub fn pci_link(device: (u32, u32), port: Option<(u32, u32)>) -> Option<PciLink> {
    let (speed, width) = port.map_or(device, |(s, w)| (device.0.min(s), device.1.min(w)));
    if !(1..=7).contains(&speed) || !(1..=32).contains(&width) {
        return None;
    }
    Some(PciLink { generation: speed as u8, lanes: width as u8 })
}

/// The Bluetooth version an LMP version number stands for.
pub fn bluetooth_version(lmp: u8) -> Option<&'static str> {
    Some(match lmp {
        6 => "4.0",
        7 => "4.1",
        8 => "4.2",
        9 => "5.0",
        10 => "5.1",
        11 => "5.2",
        12 => "5.3",
        13 => "5.4",
        14 => "6.0",
        _ => return None,
    })
}

/// Video memory the driver reports; shared-memory GPUs report little or nothing.
pub fn vram_mb(bytes: u64) -> Option<u64> {
    Some(bytes / (1024 * 1024)).filter(|&mb| mb >= 512)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Panel {
    pub maker: Option<String>,
    pub width: u32,
    pub height: u32,
    pub max_hz: f32,
    pub diagonal_in: Option<f32>,
}

/// The screen size: the preferred timing's millimetres when the base block's centimetres agree,
/// else the centimetres. Some panels put a made-up size in the timing.
fn diagonal_in(edid: &[u8]) -> Option<f32> {
    let positive = |&(w, h): &(u32, u32)| w > 0 && h > 0;
    let dtd = edid.get(54..72).filter(|d| d[0..2] != [0, 0]);
    let mm = dtd
        .map(|d| (u32::from(d[12]) | (u32::from(d[14] >> 4) << 8), u32::from(d[13]) | (u32::from(d[14] & 0xF) << 8)))
        .filter(positive);
    let cm = edid.get(21..23).map(|b| (u32::from(b[0]) * 10, u32::from(b[1]) * 10)).filter(positive);
    let (w, h) = match (mm, cm) {
        (Some(mm), Some(cm)) if mm.0.abs_diff(cm.0) > 15 || mm.1.abs_diff(cm.1) > 15 => cm,
        (mm, cm) => mm.or(cm)?,
    };
    let inches = f64::from(w * w + h * h).sqrt() / 25.4;
    Some((inches * 10.0).round() as f32 / 10.0)
}

/// The panel vendor from the EDID's three-letter PnP ID.
fn panel_maker(edid: &[u8]) -> Option<String> {
    let id = u16::from_be_bytes([*edid.get(8)?, *edid.get(9)?]);
    let letter = |shift: u16| char::from(b'A' - 1 + ((id >> shift) & 0x1F) as u8);
    let code: String = [letter(10), letter(5), letter(0)].iter().collect();
    let name = match code.as_str() {
        "LGD" => "LG Display",
        "SHP" => "Sharp",
        "AUO" => "AU Optronics",
        "BOE" => "BOE",
        "SDC" => "Samsung Display",
        "CMN" => "Innolux",
        "IVO" => "InfoVision",
        "CSO" => "CSOT",
        _ if code.chars().all(|c| c.is_ascii_uppercase()) => &code,
        _ => return None,
    };
    Some(name.to_string())
}

/// The built-in screen: its native resolution, top refresh rate there, size and maker.
pub fn panel(displays: &[RawDisplay]) -> Option<Panel> {
    let d = displays.iter().find(|d| d.connection == Connection::Internal)?;
    let edid = d.edid.as_deref().filter(|e| e.len() >= 128).unwrap_or_default();
    let modes = parse_edid(edid).modes;
    let native = modes.first().copied().unwrap_or(d.current);
    // The modes Windows offers; the EDID's timings only without them, as a variable refresh
    // range can list rates Windows never uses.
    let candidates = if d.offered.is_empty() { &modes } else { &d.offered };
    let max_hz = candidates
        .iter()
        .chain([&d.current])
        .filter(|m: &&Mode| m.width == native.width && m.height == native.height)
        .map(|m| m.hz)
        .fold(0.0, f32::max);
    let max_hz = if max_hz > 0.0 { max_hz } else { native.hz };
    Some(Panel {
        maker: panel_maker(edid),
        width: native.width,
        height: native.height,
        max_hz: max_hz.round(),
        diagonal_in: diagonal_in(edid),
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cpu {
    pub name: String,
    pub cores: u32,
    pub threads: u32,
    pub l2_kb: u32,
    pub l3_kb: u32,
    /// Instruction set extensions worth knowing about: AVX2, AVX-512 and so on.
    pub features: Vec<String>,
    /// Whether the firmware has hardware virtualization turned on.
    pub virtualization: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub name: String,
    pub driver: Option<String>,
    pub vram_mb: Option<u64>,
    pub link: Option<PciLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub name: String,
    pub size_gb: Option<u64>,
    pub link: Option<PciLink>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub model: Option<String>,
    pub bios_version: Option<String>,
    pub bios_date: Option<String>,
    pub cpu: Option<Cpu>,
    pub memory: Memory,
    pub graphics: Vec<Gpu>,
    pub storage: Vec<Disk>,
    pub panel: Option<Panel>,
    pub wifi: Option<String>,
    /// The Bluetooth version the radio supports, like "5.2".
    pub bluetooth: Option<String>,
    pub thunderbolt: Option<String>,
    /// "2.0" or "1.2".
    pub tpm: Option<String>,
    pub secure_boot: Option<bool>,
    pub fingerprint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pci_link_is_the_slower_side() {
        // T1200: Gen4 x16 capable, in a Gen4 x8 port.
        assert_eq!(pci_link((4, 16), Some((4, 8))), Some(PciLink { generation: 4, lanes: 8 }));
        // A Gen4 SSD in a Gen3 slot runs at Gen3.
        assert_eq!(pci_link((4, 4), Some((3, 4))), Some(PciLink { generation: 3, lanes: 4 }));
        assert_eq!(pci_link((3, 4), None), Some(PciLink { generation: 3, lanes: 4 }));
        assert_eq!(pci_link((0, 4), None), None);
        assert_eq!(pci_link((3, 0), None), None);
    }

    #[test]
    fn bluetooth_versions() {
        assert_eq!(bluetooth_version(11), Some("5.2"));
        assert_eq!(bluetooth_version(9), Some("5.0"));
        assert_eq!(bluetooth_version(2), None);
        assert_eq!(bluetooth_version(200), None);
    }

    #[test]
    fn vram_ignores_shared_memory_gpus() {
        assert_eq!(vram_mb(4 * 1024 * 1024 * 1024), Some(4096));
        assert_eq!(vram_mb(128 * 1024 * 1024), None);
    }

    /// A base EDID block: PnP ID, size in cm and one detailed timing with its size in mm.
    fn edid(pnp: &str, cm: (u8, u8), mm: (u16, u16), mode: (u16, u16), clock_10khz: u16) -> Vec<u8> {
        let mut e = vec![0u8; 128];
        e[..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        let b = pnp.as_bytes();
        let id = (u16::from(b[0] - b'A' + 1) << 10) | (u16::from(b[1] - b'A' + 1) << 5) | u16::from(b[2] - b'A' + 1);
        e[8..10].copy_from_slice(&id.to_be_bytes());
        (e[21], e[22]) = cm;
        let d = &mut e[54..72];
        d[0..2].copy_from_slice(&clock_10khz.to_le_bytes());
        let (h, v, hb, vb) = (mode.0, mode.1, 160u16, 40u16);
        d[2] = h as u8;
        d[3] = hb as u8;
        d[4] = (((h >> 8) << 4) | (hb >> 8)) as u8;
        d[5] = v as u8;
        d[6] = vb as u8;
        d[7] = (((v >> 8) << 4) | (vb >> 8)) as u8;
        d[12] = mm.0 as u8;
        d[13] = mm.1 as u8;
        d[14] = (((mm.0 >> 8) << 4) | (mm.1 >> 8)) as u8;
        e
    }

    fn internal(edid: Option<Vec<u8>>, current: Mode, offered: Vec<Mode>) -> RawDisplay {
        RawDisplay { name: None, connection: Connection::Internal, current, edid, offered }
    }

    #[test]
    fn panel_from_the_internal_edid() {
        // 1920 × 1200 at 60 Hz, 336 × 210 mm: 15.6".
        let clock = ((2080u64 * 1240 * 60) / 10_000) as u16;
        let e = edid("SHP", (34, 21), (336, 210), (1920, 1200), clock);
        let current = Mode { width: 1280, height: 800, hz: 60.0 };
        let p = panel(&[internal(Some(e), current, vec![])]).unwrap();
        assert_eq!(p.maker.as_deref(), Some("Sharp"));
        assert_eq!((p.width, p.height), (1920, 1200));
        assert_eq!(p.max_hz, 60.0);
        assert_eq!(p.diagonal_in, Some(15.6));
    }

    #[test]
    fn panel_top_refresh_comes_from_the_modes_windows_offers() {
        let clock = ((2080u64 * 1120 * 60) / 10_000) as u16;
        let e = edid("BOE", (34, 21), (0, 0), (1920, 1080), clock);
        let native = Mode { width: 1920, height: 1080, hz: 60.0 };
        let fast = Mode { width: 1920, height: 1080, hz: 165.0 };
        let small = Mode { width: 1280, height: 720, hz: 240.0 };
        let p = panel(&[internal(Some(e), native, vec![native, fast, small])]).unwrap();
        assert_eq!(p.max_hz, 165.0);
        // No size in the timing: the centimetres, 340 × 210 mm → 15.7".
        assert_eq!(p.diagonal_in, Some(15.7));
    }

    #[test]
    fn panel_ignores_a_timing_size_the_centimetres_contradict() {
        // Precision 5560: the timing claims 480 × 300 mm, the base block 34 × 21 cm.
        let clock = ((2080u64 * 1240 * 60) / 10_000) as u16;
        let e = edid("SHP", (34, 21), (480, 300), (1920, 1200), clock);
        let current = Mode { width: 1920, height: 1200, hz: 60.0 };
        let p = panel(&[internal(Some(e), current, vec![current])]).unwrap();
        assert_eq!(p.diagonal_in, Some(15.7));
    }

    #[test]
    fn panel_top_refresh_is_what_windows_offers_not_the_edid_range() {
        // The EDID lists a 65 Hz timing for variable refresh; Windows offers 60.
        let clock = ((2080u64 * 1240 * 65) / 10_000) as u16;
        let e = edid("SHP", (34, 21), (336, 210), (1920, 1200), clock);
        let native = Mode { width: 1920, height: 1200, hz: 60.0 };
        let p = panel(&[internal(Some(e), native, vec![native])]).unwrap();
        assert_eq!(p.max_hz, 60.0);
    }

    #[test]
    fn panel_without_edid_uses_the_current_mode() {
        let current = Mode { width: 2560, height: 1600, hz: 120.0 };
        let p = panel(&[internal(None, current, vec![])]).unwrap();
        assert_eq!((p.width, p.height, p.max_hz), (2560, 1600, 120.0));
        assert_eq!((p.maker, p.diagonal_in), (None, None));
        let external = RawDisplay { connection: Connection::Hdmi, ..internal(None, current, vec![]) };
        assert_eq!(panel(&[external]), None);
    }
}
