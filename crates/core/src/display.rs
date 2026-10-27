//! Connected displays: the mode each runs at against the best its EDID offers at its
//! native resolution, and whether a setting or the link (cable, adapter, port) holds it back.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub hz: f32,
}

impl Mode {
    fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    fn same_size(&self, other: &Mode) -> bool {
        self.width == other.width && self.height == other.height
    }
}

/// What a monitor says about itself.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Edid {
    pub name: Option<String>,
    /// Every timing it lists; the first is the preferred one, its native resolution.
    pub modes: Vec<Mode>,
}

/// A detailed timing: pixel clock in Hz and the active and blanking sizes.
fn timing(clock_hz: f64, h: u32, h_blank: u32, v: u32, v_blank: u32) -> Option<Mode> {
    let total = f64::from(h + h_blank) * f64::from(v + v_blank);
    (clock_hz > 0.0 && h > 0 && v > 0 && total > 0.0)
        .then(|| Mode { width: h, height: v, hz: ((clock_hz / total) * 100.0).round() as f32 / 100.0 })
}

/// An 18-byte detailed timing descriptor, or `None` for a display descriptor or an interlaced mode.
fn dtd(d: &[u8]) -> Option<Mode> {
    let clock = u16::from_le_bytes([d[0], d[1]]);
    if clock == 0 || d[17] & 0x80 != 0 {
        return None;
    }
    let h = u32::from(d[2]) | u32::from(d[4] >> 4) << 8;
    let h_blank = u32::from(d[3]) | u32::from(d[4] & 0xF) << 8;
    let v = u32::from(d[5]) | u32::from(d[7] >> 4) << 8;
    let v_blank = u32::from(d[6]) | u32::from(d[7] & 0xF) << 8;
    timing(f64::from(clock) * 10_000.0, h, h_blank, v, v_blank)
}

/// The monitor name display descriptor (tag 0xFC).
fn name(d: &[u8]) -> Option<String> {
    if d[..3] != [0, 0, 0] || d[3] != 0xFC {
        return None;
    }
    let text: String = d[5..18].iter().take_while(|&&b| b != 0x0A && b != 0).map(|&b| b as char).collect();
    Some(text.trim().to_string()).filter(|s| !s.is_empty())
}

/// CTA-861 short video descriptors this check cares about: the common TV and monitor modes.
fn vic(code: u8) -> Option<Mode> {
    let (width, height, hz) = match code {
        4 => (1280, 720, 60.0),
        16 => (1920, 1080, 60.0),
        31 => (1920, 1080, 50.0),
        32 => (1920, 1080, 24.0),
        33 => (1920, 1080, 25.0),
        34 => (1920, 1080, 30.0),
        63 => (1920, 1080, 120.0),
        64 => (1920, 1080, 100.0),
        93 => (3840, 2160, 24.0),
        94 => (3840, 2160, 25.0),
        95 => (3840, 2160, 30.0),
        96 => (3840, 2160, 50.0),
        97 => (3840, 2160, 60.0),
        117 => (3840, 2160, 100.0),
        118 => (3840, 2160, 120.0),
        _ => return None,
    };
    Some(Mode { width, height, hz })
}

fn cta(block: &[u8], modes: &mut Vec<Mode>) {
    let dtd_start = usize::from(block[2]).min(127);
    let mut pos = 4;
    while pos < dtd_start {
        let (tag, len) = (block[pos] >> 5, usize::from(block[pos] & 0x1F));
        let end = (pos + 1 + len).min(dtd_start);
        if tag == 2 {
            for &svd in &block[pos + 1..end] {
                // 1–64 and 129–192 carry a "native" flag in the top bit.
                let code = if matches!(svd, 1..=64 | 129..=192) { svd & 0x7F } else { svd };
                modes.extend(vic(code));
            }
        }
        pos = end;
    }
    if dtd_start >= 4 {
        let mut pos = dtd_start;
        while pos + 18 <= 127 && block[pos..pos + 2] != [0, 0] {
            modes.extend(dtd(&block[pos..pos + 18]));
            pos += 18;
        }
    }
}

/// DisplayID Type I (1.x, 10 kHz clock) and Type VII (2.0, 1 kHz clock) detailed timings.
fn display_id(block: &[u8], modes: &mut Vec<Mode>) {
    let end = (5 + usize::from(block[2])).min(127);
    let mut pos = 5;
    while pos + 3 <= end {
        let (tag, len) = (block[pos], usize::from(block[pos + 2]));
        let payload = &block[pos + 3..(pos + 3 + len).min(end)];
        let clock_unit = match tag {
            0x03 => Some(10_000.0),
            0x22 => Some(1_000.0),
            _ => None,
        };
        if let Some(unit) = clock_unit {
            for t in payload.as_chunks::<20>().0 {
                if t[3] & 0x10 != 0 {
                    continue; // interlaced
                }
                let word = |i: usize| u32::from(u16::from_le_bytes([t[i], t[i + 1]])) + 1;
                let clock = f64::from(u32::from_le_bytes([t[0], t[1], t[2], 0]) + 1) * unit;
                modes.extend(timing(clock, word(4), word(6), word(12), word(14)));
            }
        }
        pos += 3 + len;
    }
}

/// Parses a base EDID block and its CTA-861 and DisplayID extensions. Whatever is malformed is skipped.
pub fn parse_edid(raw: &[u8]) -> Edid {
    let mut edid = Edid::default();
    if raw.len() < 128 || raw[..8] != [0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0] {
        return edid;
    }
    for d in raw[54..126].as_chunks::<18>().0 {
        edid.modes.extend(dtd(d));
        edid.name = edid.name.or_else(|| name(d));
    }
    for block in raw[128..].as_chunks::<128>().0 {
        match block[0] {
            0x02 => cta(block, &mut edid.modes),
            0x70 => display_id(block, &mut edid.modes),
            _ => {}
        }
    }
    edid
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Connection {
    Internal,
    DisplayPort,
    Hdmi,
    Dvi,
    Vga,
    /// A DisplayLink-style dock or adapter that sends the picture over USB.
    Usb,
    Wireless,
    Other,
}

/// One active display as Windows reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawDisplay {
    /// Windows' name for the monitor, used when the EDID has none.
    pub name: Option<String>,
    pub connection: Connection,
    pub current: Mode,
    pub edid: Option<Vec<u8>>,
    /// The modes Windows offers for it right now. The driver leaves out what the link can't carry.
    pub offered: Vec<Mode>,
}

/// What keeps a display below its best mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Limit {
    /// Windows offers the better mode: it's a setting away.
    Setting,
    /// Windows doesn't offer it, so the cable, adapter or port can't carry it.
    Link,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Upgrade {
    pub mode: Mode,
    pub limit: Limit,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    pub name: Option<String>,
    pub connection: Connection,
    pub current: Mode,
    /// Set when the display could run at a higher resolution or refresh rate.
    pub upgrade: Option<Upgrade>,
}

/// A higher refresh rate counts from this much faster, so 60 against a 59.94 timing,
/// or a laptop panel's extra 65 Hz timing, isn't flagged.
const FASTER: f32 = 1.1;

/// The best mode at the native resolution, when it beats the current one.
fn upgrade(d: &RawDisplay, edid: &Edid) -> Option<Upgrade> {
    let native = edid.modes.first()?;
    let best = edid
        .modes
        .iter()
        .filter(|m| m.same_size(native))
        .max_by(|a, b| a.hz.total_cmp(&b.hz))
        .copied()?;
    let sharper = best.pixels() > d.current.pixels();
    let faster = best.same_size(&d.current) && best.hz >= d.current.hz * FASTER;
    if !sharper && !faster {
        return None;
    }
    // Windows lists whole hertz: 60 for a 59.94 timing.
    let offered = d.offered.iter().any(|m| m.same_size(&best) && (m.hz - best.hz).abs() < 1.5);
    Some(Upgrade { mode: best, limit: if offered { Limit::Setting } else { Limit::Link } })
}

/// The built-in screen first. Laptop panels list odd timings for power saving, so only
/// external displays are checked for an upgrade.
pub fn report(raw: Vec<RawDisplay>) -> Vec<Display> {
    let mut out: Vec<Display> = raw
        .into_iter()
        .map(|d| {
            let edid = d.edid.as_deref().map(parse_edid).unwrap_or_default();
            let upgrade = if d.connection == Connection::Internal { None } else { upgrade(&d, &edid) };
            Display { name: edid.name.or(d.name), connection: d.connection, current: d.current, upgrade }
        })
        .collect();
    out.sort_by_key(|d| d.connection != Connection::Internal);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This laptop's own 1920×1200 panel.
    const PANEL: &str = "00ffffffffffff0030e4b30600000000001f0104a522157806a205a65449a2250e5054000000010101010101010101010101010101013a4180a070b0234030203600e02c1100001a283c80a070b0234030203600e02c1100001a203080a070b0234030203600e02c1100001a00000010000000000000000000000000000000e9";

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn mode(width: u32, height: u32, hz: f32) -> Mode {
        Mode { width, height, hz }
    }

    /// An 18-byte detailed timing descriptor.
    fn dtd_bytes(clock_10khz: u16, h: u32, h_blank: u32, v: u32, v_blank: u32) -> [u8; 18] {
        let mut d = [0u8; 18];
        d[..2].copy_from_slice(&clock_10khz.to_le_bytes());
        d[2] = h as u8;
        d[3] = h_blank as u8;
        d[4] = ((h >> 8) << 4 | (h_blank >> 8)) as u8;
        d[5] = v as u8;
        d[6] = v_blank as u8;
        d[7] = ((v >> 8) << 4 | (v_blank >> 8)) as u8;
        d
    }

    fn name_bytes(name: &str) -> [u8; 18] {
        let mut d = [0u8; 18];
        d[3] = 0xFC;
        d[5..18].fill(0x20);
        d[5..5 + name.len()].copy_from_slice(name.as_bytes());
        d[5 + name.len()] = 0x0A;
        d
    }

    /// A base block with these descriptors and `ext` extension blocks after it.
    fn edid(descriptors: &[[u8; 18]], ext: &[[u8; 128]]) -> Vec<u8> {
        let mut b = vec![0u8; 128];
        b[..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
        for (i, d) in descriptors.iter().enumerate() {
            b[54 + i * 18..72 + i * 18].copy_from_slice(d);
        }
        b[126] = ext.len() as u8;
        for e in ext {
            b.extend_from_slice(e);
        }
        b
    }

    /// 3840×2160 at 60 Hz, CVT reduced blanking.
    fn uhd60() -> [u8; 18] {
        dtd_bytes(53325, 3840, 160, 2160, 62)
    }

    #[test]
    fn reads_this_laptops_panel() {
        let e = parse_edid(&hex(PANEL));
        assert_eq!(e.name, None);
        assert_eq!(e.modes, [mode(1920, 1200, 65.0), mode(1920, 1200, 59.95), mode(1920, 1200, 47.96)]);
    }

    #[test]
    fn reads_the_name_and_preferred_timing() {
        let e = parse_edid(&edid(&[uhd60(), name_bytes("DELL U2720Q")], &[]));
        assert_eq!(e.name.as_deref(), Some("DELL U2720Q"));
        assert_eq!(e.modes, [mode(3840, 2160, 60.0)]);
    }

    #[test]
    fn reads_cta_video_codes_and_timings() {
        let mut cta = [0u8; 128];
        cta[0] = 0x02;
        cta[1] = 3;
        // One video data block (tag 2): 4K60, 1080p60 flagged native, and a code this doesn't know.
        cta[4..8].copy_from_slice(&[2 << 5 | 3, 97, 128 | 16, 200]);
        cta[2] = 8;
        cta[8..26].copy_from_slice(&dtd_bytes(14850, 1920, 280, 1080, 45));
        let e = parse_edid(&edid(&[uhd60()], &[cta]));
        assert_eq!(e.modes, [mode(3840, 2160, 60.0), mode(3840, 2160, 60.0), mode(1920, 1080, 60.0), mode(1920, 1080, 60.0)]);
    }

    #[test]
    fn reads_display_id_timings() {
        // A 1440p 165 Hz monitor that lists its fastest mode only in DisplayID.
        let mut t = [0u8; 20];
        let clock = 645_000_u32 / 10 - 1; // 645 MHz in 10 kHz units, stored minus one
        t[..3].copy_from_slice(&clock.to_le_bytes()[..3]);
        let word = |t: &mut [u8; 20], i: usize, v: u32| t[i..i + 2].copy_from_slice(&((v - 1) as u16).to_le_bytes());
        word(&mut t, 4, 2560);
        word(&mut t, 6, 80);
        word(&mut t, 12, 1440);
        word(&mut t, 14, 41);
        let mut id = [0u8; 128];
        id[0] = 0x70;
        id[1] = 0x12;
        id[2] = 23;
        id[5..8].copy_from_slice(&[0x03, 0, 20]);
        id[8..28].copy_from_slice(&t);
        let e = parse_edid(&edid(&[dtd_bytes(24150, 2560, 160, 1440, 41)], &[id]));
        assert_eq!(e.modes, [mode(2560, 1440, 59.95), mode(2560, 1440, 164.97)]);
    }

    #[test]
    fn garbage_yields_nothing() {
        assert_eq!(parse_edid(&[]), Edid::default());
        assert_eq!(parse_edid(&[0xFF; 300]), Edid::default());
        // A valid header with a truncated extension and a CTA block pointing past its end.
        let mut cta = [0xFFu8; 128];
        cta[0] = 0x02;
        let mut raw = edid(&[uhd60()], &[cta]);
        raw.truncate(200);
        assert_eq!(parse_edid(&raw).modes, [mode(3840, 2160, 60.0)]);
        let _ = parse_edid(&edid(&[uhd60()], &[cta]));
    }

    fn external(current: Mode, offered: &[Mode]) -> RawDisplay {
        RawDisplay {
            name: Some("DELL U2720Q".into()),
            connection: Connection::DisplayPort,
            current,
            edid: Some(edid(&[uhd60(), name_bytes("DELL U2720Q")], &[])),
            offered: offered.to_vec(),
        }
    }

    #[test]
    fn a_4k_monitor_at_30_hz_blames_the_link_when_windows_has_no_60() {
        let d = report(vec![external(mode(3840, 2160, 30.0), &[mode(3840, 2160, 30.0)])]);
        assert_eq!(d[0].upgrade, Some(Upgrade { mode: mode(3840, 2160, 60.0), limit: Limit::Link }));
    }

    #[test]
    fn a_mode_windows_offers_is_a_setting_away() {
        let offered = [mode(3840, 2160, 30.0), mode(3840, 2160, 60.0)];
        let d = report(vec![external(mode(3840, 2160, 30.0), &offered)]);
        assert_eq!(d[0].upgrade.map(|u| u.limit), Some(Limit::Setting));
        // Below native resolution counts too.
        let d = report(vec![external(mode(2560, 1440, 60.0), &[mode(3840, 2160, 60.0)])]);
        assert_eq!(d[0].upgrade, Some(Upgrade { mode: mode(3840, 2160, 60.0), limit: Limit::Setting }));
    }

    #[test]
    fn running_at_its_best_is_fine() {
        assert_eq!(report(vec![external(mode(3840, 2160, 60.0), &[])])[0].upgrade, None);
        assert_eq!(report(vec![external(mode(3840, 2160, 59.0), &[])])[0].upgrade, None);
    }

    #[test]
    fn the_built_in_panel_is_listed_first_and_never_flagged() {
        let panel = RawDisplay {
            name: None,
            connection: Connection::Internal,
            current: mode(1920, 1200, 60.0),
            edid: Some(hex(PANEL)),
            offered: vec![],
        };
        let d = report(vec![external(mode(3840, 2160, 60.0), &[]), panel]);
        assert_eq!(d[0].connection, Connection::Internal);
        assert_eq!(d[0].upgrade, None);
        assert_eq!(d[1].name.as_deref(), Some("DELL U2720Q"));
    }

    #[test]
    fn without_an_edid_windows_name_is_kept_and_nothing_is_guessed() {
        let mut d = external(mode(1280, 720, 60.0), &[]);
        d.edid = None;
        d.name = Some("Generic PnP Monitor".into());
        let r = report(vec![d]);
        assert_eq!((r[0].name.as_deref(), r[0].upgrade), (Some("Generic PnP Monitor"), None));
    }
}
