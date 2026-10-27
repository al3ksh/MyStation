//! Device identity from the raw SMBIOS table (`GetSystemFirmwareTable('RSMB')`).

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    /// Dell's service tag is the SMBIOS system serial number.
    pub service_tag: Option<String>,
    pub bios_version: Option<String>,
    pub bios_date: Option<String>,
}

/// One structure: its type, formatted area (header included) and strings.
struct Structure<'a> {
    kind: u8,
    formatted: &'a [u8],
    strings: Vec<&'a [u8]>,
}

impl Structure<'_> {
    fn string(&self, offset: usize) -> Option<String> {
        string_at(self.formatted, &self.strings, offset)
    }

    fn byte(&self, offset: usize) -> Option<u8> {
        self.formatted.get(offset).copied()
    }

    fn word(&self, offset: usize) -> Option<u16> {
        self.formatted.get(offset..offset + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn dword(&self, offset: usize) -> Option<u32> {
        self.formatted.get(offset..offset + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn qword(&self, offset: usize) -> Option<u64> {
        self.formatted.get(offset..offset + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }
}

/// Walks the buffer `GetSystemFirmwareTable('RSMB')` returns: an 8-byte header
/// (`RawSMBIOSData`) followed by the structure table. Stops at the first malformed structure.
fn structures(raw: &[u8]) -> Vec<Structure<'_>> {
    let mut out = Vec::new();
    let Some(len) = raw.get(4..8).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) else {
        return out;
    };
    let table = &raw[8..raw.len().min(8 + len)];

    let mut pos = 0;
    while pos + 4 <= table.len() {
        let kind = table[pos];
        let formatted_len = table[pos + 1] as usize;
        if formatted_len < 4 || pos + formatted_len > table.len() || kind == 127 {
            break;
        }
        let strings_start = pos + formatted_len;
        let strings_end = find_double_nul(table, strings_start).unwrap_or(table.len());
        out.push(Structure {
            kind,
            formatted: &table[pos..pos + formatted_len],
            strings: table[strings_start..strings_end].split(|&b| b == 0).collect(),
        });
        pos = strings_end + 2;
    }
    out
}

/// Device identity from the BIOS (type 0) and System (type 1) structures. Malformed input
/// yields what was readable.
pub fn parse(raw: &[u8]) -> DeviceInfo {
    let mut info = DeviceInfo::default();
    for s in structures(raw) {
        match s.kind {
            0 => {
                info.bios_version = s.string(0x05);
                info.bios_date = s.string(0x08);
            }
            1 => {
                info.manufacturer = s.string(0x04);
                info.model = s.string(0x05);
                info.service_tag = s.string(0x07);
            }
            _ => {}
        }
    }
    info
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryModule {
    pub slot: String,
    pub size_mb: u32,
    /// DDR4, LPDDR5 and so on.
    pub kind: Option<String>,
    /// The module's rated speed, MT/s.
    pub speed_mts: Option<u32>,
    /// The speed it runs at, MT/s.
    pub configured_mts: Option<u32>,
    pub maker: Option<String>,
    pub part: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    /// The most memory the board takes.
    pub max_gb: Option<u32>,
    pub slots: Option<u32>,
    /// Installed modules; empty slots are left out.
    pub modules: Vec<MemoryModule>,
}

/// SMBIOS 3.x Memory Device type codes.
fn memory_kind(code: u8) -> Option<&'static str> {
    Some(match code {
        0x12 => "DDR",
        0x13 => "DDR2",
        0x18 => "DDR3",
        0x1A => "DDR4",
        0x1B => "LPDDR",
        0x1C => "LPDDR2",
        0x1D => "LPDDR3",
        0x1E => "LPDDR4",
        0x22 => "DDR5",
        0x23 => "LPDDR5",
        _ => return None,
    })
}

/// The maker string, with the JEDEC codes many BIOSes put there turned into names.
pub fn maker(raw: &str) -> Option<String> {
    // Some tables pad the JEDEC code to 12 hex digits, "0198000080CE": the maker is the first 4.
    let upper = raw.to_ascii_uppercase();
    let code = if upper.len() == 12 && upper.chars().all(|c| c.is_ascii_hexdigit()) { &upper[..4] } else { &upper };
    let name = match code {
        "" | "UNKNOWN" | "NOT SPECIFIED" => return None,
        "80CE" | "CE00" | "00CE" => "Samsung",
        "80AD" | "AD00" | "00AD" => "SK hynix",
        "802C" | "2C00" | "002C" => "Micron",
        "0198" | "9801" => "Kingston",
        "859B" | "9B05" => "Crucial",
        _ => raw,
    };
    Some(name.to_string())
}

/// Speeds of 0 are unknown; 0xFFFF points at an extended field older tables don't have.
fn speed(word: Option<u16>) -> Option<u32> {
    word.filter(|&w| w != 0 && w != 0xFFFF).map(u32::from)
}

fn module(s: &Structure) -> Option<MemoryModule> {
    let size_mb = match s.word(0x0C)? {
        0 | 0xFFFF => return None,
        0x7FFF => s.dword(0x1C)? & 0x7FFF_FFFF,
        w if w & 0x8000 != 0 => u32::from(w & 0x7FFF) / 1024,
        w => u32::from(w),
    };
    Some(MemoryModule {
        slot: s.string(0x10).unwrap_or_default(),
        size_mb,
        kind: s.byte(0x12).and_then(memory_kind).map(str::to_string),
        speed_mts: speed(s.word(0x15)),
        configured_mts: speed(s.word(0x20)),
        maker: s.string(0x17).and_then(|m| maker(&m)),
        part: s.string(0x1A),
    })
}

/// The memory arrays (type 16) and modules (type 17).
pub fn memory(raw: &[u8]) -> Memory {
    let mut mem = Memory::default();
    for s in structures(raw) {
        match s.kind {
            // Use 3 is system memory, as opposed to video or flash.
            16 if s.byte(0x05) == Some(3) => {
                let kb = match s.dword(0x07) {
                    Some(0x8000_0000) => s.qword(0x0F).map(|b| b / 1024),
                    kb => kb.map(u64::from),
                };
                if let Some(gb) = kb.map(|kb| (kb / 1024 / 1024) as u32).filter(|&gb| gb > 0) {
                    *mem.max_gb.get_or_insert(0) += gb;
                }
                if let Some(n) = s.word(0x0D) {
                    *mem.slots.get_or_insert(0) += u32::from(n);
                }
            }
            17 => mem.modules.extend(module(&s)),
            _ => {}
        }
    }
    mem
}

/// The string set ends with two NULs (an empty set is just the two NULs).
fn find_double_nul(table: &[u8], from: usize) -> Option<usize> {
    (from..table.len().saturating_sub(1)).find(|&i| table[i] == 0 && table[i + 1] == 0)
}

fn string_at(formatted: &[u8], strings: &[&[u8]], offset: usize) -> Option<String> {
    let index = *formatted.get(offset)? as usize;
    let bytes = strings.get(index.checked_sub(1)?)?;
    let s = String::from_utf8_lossy(bytes).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure(kind: u8, formatted: &[u8], strings: &[&str]) -> Vec<u8> {
        let mut v = vec![kind, (formatted.len() + 4) as u8, 0, 0];
        v.extend_from_slice(formatted);
        if strings.is_empty() {
            v.push(0);
        }
        for s in strings {
            v.extend_from_slice(s.as_bytes());
            v.push(0);
        }
        v.push(0);
        v
    }

    fn rsmb(structures: &[Vec<u8>]) -> Vec<u8> {
        let table: Vec<u8> = structures.concat();
        let mut v = vec![0, 3, 4, 0];
        v.extend_from_slice(&(table.len() as u32).to_le_bytes());
        v.extend(table);
        v
    }

    #[test]
    fn reads_model_service_tag_and_bios() {
        // Type 0: vendor=1, version=2, start segment (2 bytes), date=3.
        let bios = structure(0, &[1, 2, 0, 0xF0, 3], &["Dell Inc.", "1.31.0", "06/12/2025"]);
        // Type 1: manufacturer=1, product=2, version=0, serial=3.
        let system = structure(1, &[1, 2, 0, 3], &["Dell Inc.", "Precision 5560", "ABC1234"]);
        let end = structure(127, &[], &[]);
        let info = parse(&rsmb(&[bios, system, end]));
        assert_eq!(
            info,
            DeviceInfo {
                manufacturer: Some("Dell Inc.".into()),
                model: Some("Precision 5560".into()),
                service_tag: Some("ABC1234".into()),
                bios_version: Some("1.31.0".into()),
                bios_date: Some("06/12/2025".into()),
            }
        );
    }

    #[test]
    fn skips_other_structures_and_structures_without_strings() {
        let other = structure(4, &[0, 0, 0, 0], &[]);
        let system = structure(1, &[0, 1, 0, 0], &["Precision 5560"]);
        let info = parse(&rsmb(&[other, system]));
        assert_eq!(info.model.as_deref(), Some("Precision 5560"));
        assert_eq!(info.service_tag, None);
        assert_eq!(info.manufacturer, None);
    }

    #[test]
    fn blank_and_truncated_input_yield_nothing() {
        assert_eq!(parse(&[]), DeviceInfo::default());
        assert_eq!(parse(&[0, 3, 4, 0, 50, 0, 0, 0, 1, 99]), DeviceInfo::default());
        let system = structure(1, &[1, 2, 0, 3], &["  ", "Precision 5560", ""]);
        let info = parse(&rsmb(&[system]));
        assert_eq!(info.manufacturer, None);
        assert_eq!(info.model.as_deref(), Some("Precision 5560"));
    }

    /// Type 17 laid out to SMBIOS 3.x: size at 0x0C, locator 0x10, type 0x12, speed 0x15,
    /// maker 0x17, part 0x1A, extended size 0x1C, configured speed 0x20.
    fn dimm(size_mb: u16, kind: u8, speed: u16, configured: u16, strings: &[&str]) -> Vec<u8> {
        let mut f = vec![0u8; 0x22 - 4];
        let mut put = |at: usize, bytes: &[u8]| f[at - 4..at - 4 + bytes.len()].copy_from_slice(bytes);
        put(0x0C, &size_mb.to_le_bytes());
        put(0x10, &[1]);
        put(0x11, &[2]);
        put(0x12, &[kind]);
        put(0x15, &speed.to_le_bytes());
        put(0x17, &[3]);
        put(0x1A, &[4]);
        put(0x20, &configured.to_le_bytes());
        structure(17, &f, strings)
    }

    #[test]
    fn reads_memory_slots_and_modules() {
        // Type 16: location, use, ECC, max capacity in KB (64 GB), handle, slot count.
        let mut array = vec![3, 3, 3];
        array.extend_from_slice(&(64u32 * 1024 * 1024).to_le_bytes());
        array.extend_from_slice(&[0xFE, 0xFF, 2, 0]);
        let a = dimm(16384, 0x1A, 3200, 3200, &["DIMM A", "BANK 0", "0198", "9905744-108.A00G  "]);
        let b = dimm(0, 0, 0, 0, &["DIMM B", "BANK 2"]);
        let mem = memory(&rsmb(&[structure(16, &array, &[]), a, b]));
        assert_eq!(mem.max_gb, Some(64));
        assert_eq!(mem.slots, Some(2));
        assert_eq!(
            mem.modules,
            vec![MemoryModule {
                slot: "DIMM A".into(),
                size_mb: 16384,
                kind: Some("DDR4".into()),
                speed_mts: Some(3200),
                configured_mts: Some(3200),
                maker: Some("Kingston".into()),
                part: Some("9905744-108.A00G".into()),
            }]
        );
    }

    #[test]
    fn memory_sizes_in_kb_and_extended_sizes() {
        let mut kb = dimm(0x8000 | 512, 0x22, 0, 0, &["A", "BANK 0", "Samsung", "M1"]);
        let mut ext = dimm(0x7FFF, 0x23, 6400, 0xFFFF, &["B", "BANK 1", "80AD", "H1"]);
        // Extended size (MB) at 0x1C, after the 4-byte header written by `structure`.
        ext[0x1C..0x20].copy_from_slice(&65536u32.to_le_bytes());
        kb[0x15..0x17].copy_from_slice(&0u16.to_le_bytes());
        let mem = memory(&rsmb(&[kb, ext]));
        assert_eq!(mem.modules[0].size_mb, 0);
        assert_eq!(mem.modules[0].kind.as_deref(), Some("DDR5"));
        assert_eq!(mem.modules[0].maker.as_deref(), Some("Samsung"));
        assert_eq!(mem.modules[0].speed_mts, None);
        assert_eq!(mem.modules[1].size_mb, 65536);
        assert_eq!(mem.modules[1].kind.as_deref(), Some("LPDDR5"));
        assert_eq!(mem.modules[1].maker.as_deref(), Some("SK hynix"));
        assert_eq!(mem.modules[1].configured_mts, None);
        assert_eq!(mem.max_gb, None);
    }

    #[test]
    fn jedec_makers_and_unknown_codes() {
        assert_eq!(maker("802C").as_deref(), Some("Micron"));
        assert_eq!(maker("CE00").as_deref(), Some("Samsung"));
        assert_eq!(maker("859B").as_deref(), Some("Crucial"));
        assert_eq!(maker("Unknown"), None);
        assert_eq!(maker("A-DATA"), Some("A-DATA".into()));
        assert_eq!(maker("0198000080CE").as_deref(), Some("Kingston"));
    }
}
