//! Active displays from the display configuration: connection, current mode, the monitor's EDID
//! from its device key, and the modes the driver offers for it. Nothing here changes a setting.

use mystation_core::display::{Connection, Mode, RawDisplay};
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_MODE_INFO_TYPE_TARGET, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EXTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_USB_TUNNEL,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DVI, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HD15, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_WIRED, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_MIRACAST, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_RATIONAL, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY, DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
use windows::Win32::Graphics::Gdi::{DEVMODEW, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplaySettingsW};
use windows::core::PCWSTR;

use super::registry::hklm_binary;

fn utf16(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn hz(r: DISPLAYCONFIG_RATIONAL) -> f32 {
    if r.Denominator == 0 { 0.0 } else { ((f64::from(r.Numerator) / f64::from(r.Denominator)) * 100.0).round() as f32 / 100.0 }
}

fn connection(tech: DISPLAYCONFIG_VIDEO_OUTPUT_TECHNOLOGY) -> Connection {
    match tech {
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
        | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS => Connection::Internal,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EXTERNAL | DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_USB_TUNNEL => {
            Connection::DisplayPort
        }
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HDMI => Connection::Hdmi,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DVI => Connection::Dvi,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_HD15 => Connection::Vga,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INDIRECT_WIRED => Connection::Usb,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_MIRACAST => Connection::Wireless,
        _ => Connection::Other,
    }
}

/// `\\?\DISPLAY#LGD06B3#4&26cafddf&0&UID8388688#{e6f07b5f-…}` → `DISPLAY\LGD06B3\4&26cafddf&0&UID8388688`.
pub fn instance_id(device_path: &str) -> Option<String> {
    let rest = device_path.strip_prefix(r"\\?\")?;
    let end = rest.rfind("#{")?;
    Some(rest[..end].replace('#', r"\"))
}

/// The EDID Windows cached from the monitor when it was connected.
fn edid(device_path: &str) -> Option<Vec<u8>> {
    let id = instance_id(device_path)?;
    hklm_binary(&format!(r"SYSTEM\CurrentControlSet\Enum\{id}\Device Parameters"), "EDID")
}

/// Every mode the driver lists for a GDI display like `\\.\DISPLAY1`.
fn offered(gdi_name: &[u16]) -> Vec<Mode> {
    let mut out = Vec::new();
    for i in 0.. {
        let mut mode = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..Default::default() };
        // SAFETY: `gdi_name` is NUL-terminated and `mode` is live with its size set.
        if !unsafe { EnumDisplaySettingsW(PCWSTR(gdi_name.as_ptr()), ENUM_DISPLAY_SETTINGS_MODE(i), &mut mode) }.as_bool() {
            break;
        }
        let m = Mode { width: mode.dmPelsWidth, height: mode.dmPelsHeight, hz: mode.dmDisplayFrequency as f32 };
        if !out.contains(&m) {
            out.push(m);
        }
    }
    out
}

/// # Safety
/// `path` and `modes` must come from one successful `QueryDisplayConfig` call.
unsafe fn display(path: &DISPLAYCONFIG_PATH_INFO, modes: &[DISPLAYCONFIG_MODE_INFO]) -> Option<RawDisplay> {
    unsafe {
        let target = &path.targetInfo;
        let mut name = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
        name.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
        name.header.adapterId = target.adapterId;
        name.header.id = target.id;
        let named = DisplayConfigGetDeviceInfo(&mut name.header) == ERROR_SUCCESS.0 as i32;

        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        source.header.size = size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        source.header.adapterId = path.sourceInfo.adapterId;
        source.header.id = path.sourceInfo.id;
        let gdi = DisplayConfigGetDeviceInfo(&mut source.header) == ERROR_SUCCESS.0 as i32;

        // The signal on the wire; the source mode (desktop size) and path rate are the fallback.
        let mode_at = |idx: u32| modes.get(idx as usize);
        let current = mode_at(target.Anonymous.modeInfoIdx)
            .filter(|m| m.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_TARGET)
            .map(|m| {
                let s = m.Anonymous.targetMode.targetVideoSignalInfo;
                Mode { width: s.activeSize.cx, height: s.activeSize.cy, hz: hz(s.vSyncFreq) }
            })
            .or_else(|| {
                mode_at(path.sourceInfo.Anonymous.modeInfoIdx)
                    .filter(|m| m.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE)
                    .map(|m| {
                        let s = m.Anonymous.sourceMode;
                        Mode { width: s.width, height: s.height, hz: hz(target.refreshRate) }
                    })
            })?;

        let device_path = named.then(|| utf16(&name.monitorDevicePath));
        Some(RawDisplay {
            name: named.then(|| utf16(&name.monitorFriendlyDeviceName)).filter(|n| !n.trim().is_empty()),
            connection: connection(target.outputTechnology),
            current,
            edid: device_path.as_deref().and_then(edid),
            offered: if gdi { offered(&source.viewGdiDeviceName) } else { Vec::new() },
        })
    }
}

/// The displays showing the desktop right now.
pub fn displays() -> Vec<RawDisplay> {
    // SAFETY: the arrays are sized by GetDisplayConfigBufferSizes and only read after a successful query.
    unsafe {
        // A display plugged in between the two calls changes the sizes; try again then.
        for _ in 0..3 {
            let (mut path_count, mut mode_count) = (0u32, 0u32);
            if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) != ERROR_SUCCESS {
                return Vec::new();
            }
            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
            let read = QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            );
            if read == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if read != ERROR_SUCCESS {
                return Vec::new();
            }
            paths.truncate(path_count as usize);
            modes.truncate(mode_count as usize);
            return paths.iter().filter_map(|p| display(p, &modes)).collect();
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_path_to_instance_id() {
        let path = r"\\?\DISPLAY#LGD06B3#4&26cafddf&0&UID8388688#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        assert_eq!(instance_id(path).as_deref(), Some(r"DISPLAY\LGD06B3\4&26cafddf&0&UID8388688"));
        assert_eq!(instance_id("garbage"), None);
    }

    #[test]
    fn reads_the_active_displays() {
        // Every machine running the tests shows a desktop on at least one display.
        let d = displays();
        assert!(!d.is_empty());
        assert!(d.iter().all(|d| d.current.width > 0 && d.current.hz > 0.0), "{d:?}");
        assert!(d.iter().any(|d| !d.offered.is_empty()), "{d:?}");
    }
}
