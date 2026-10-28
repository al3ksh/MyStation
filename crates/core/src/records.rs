//! All-time highs (hottest CPU, fastest fan, biggest battery draw, longest time unplugged) and
//! the current time on battery, from Windows' power source changes (Kernel-Power event 105).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::battery::{BatterySnapshot, PowerState};
use crate::boot::parse_event;
use crate::config::write_atomic;
use crate::sensors::{GpuSnapshot, Telemetry};

pub const POWER_PROVIDER: &str = "Microsoft-Windows-Kernel-Power";
/// Logged on every plug in and unplug, with the charge at that moment.
pub const POWER_SOURCE_EVENT: u32 = 105;

/// Readings past these are sensor glitches, not records.
const MAX_TEMP_C: f32 = 110.0;
const MAX_FAN_RPM: f32 = 12_000.0;
const MAX_DRAW_W: f32 = 250.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub value: f32,
    pub at_ms: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Records {
    pub cpu_temp_c: Option<Record>,
    pub gpu_temp_c: Option<Record>,
    pub fan_rpm: Option<Record>,
    /// The laptop's whole draw from the battery.
    pub battery_draw_w: Option<Record>,
    /// Minutes; `at_ms` is when that time on battery began.
    pub battery_stint_min: Option<Record>,
}

/// Raises `slot` to `value` if it beats it; true when it did.
fn raise(slot: &mut Option<Record>, value: f32, max: f32, at_ms: i64) -> bool {
    if !(value > 0.0 && value <= max) || slot.is_some_and(|r| r.value >= value) {
        return false;
    }
    *slot = Some(Record { value, at_ms });
    true
}

impl Records {
    /// True when any record moved.
    pub fn observe(&mut self, t: &Telemetry) -> bool {
        let mut moved = false;
        if let Some(c) = t.cpu.temp_c {
            moved |= raise(&mut self.cpu_temp_c, c, MAX_TEMP_C, t.ts_ms);
        }
        if let GpuSnapshot::Active { temp_c, .. } = t.gpu {
            moved |= raise(&mut self.gpu_temp_c, temp_c as f32, MAX_TEMP_C, t.ts_ms);
        }
        if let Some(rpm) = t.fans.iter().map(|f| f.rpm).max() {
            moved |= raise(&mut self.fan_rpm, rpm as f32, MAX_FAN_RPM, t.ts_ms);
        }
        if let Some(b) = &t.battery {
            moved |= self.observe_battery(b, t.ts_ms);
        }
        moved
    }

    /// The battery alone: the tray-only poll reads nothing else.
    pub fn observe_battery(&mut self, b: &BatterySnapshot, at_ms: i64) -> bool {
        b.state == PowerState::Discharging && raise(&mut self.battery_draw_w, -b.power_w, MAX_DRAW_W, at_ms)
    }

    /// A finished stint from the event log, kept if it beats the record.
    pub fn seed_stint(&mut self, r: Record) -> bool {
        raise(&mut self.battery_stint_min, r.value, f32::MAX, r.at_ms)
    }

    /// Counts the time on battery so far, so a long stint shows before it ends.
    pub fn observe_stint(&mut self, stint: &BatteryStint, now_ms: i64) -> bool {
        let min = ((now_ms - stint.since_ms) / 60_000) as f32;
        raise(&mut self.battery_stint_min, min, f32::MAX, stint.since_ms)
    }
}

/// Since when the laptop runs on battery, and its charge then.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryStint {
    pub since_ms: i64,
    /// `None` when the laptop started on battery: the charge at unplugging isn't known.
    pub since_pct: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerSourceEvent {
    pub at_ms: i64,
    pub ac_online: bool,
    pub pct: Option<f32>,
}

/// A rendered Kernel-Power 105 event.
pub fn power_source_event(xml: &str) -> Option<PowerSourceEvent> {
    let ev = parse_event(xml).filter(|e| e.id == POWER_SOURCE_EVENT)?;
    let num = |key: &str| ev.data.get(key).and_then(|v| v.parse::<f32>().ok());
    let pct = match (num("RemainingCapacity"), num("FullChargeCapacity")) {
        (Some(left), Some(full)) if full > 0.0 => Some((left / full * 100.0).min(100.0)),
        _ => None,
    };
    Some(PowerSourceEvent {
        at_ms: ev.time?.timestamp_millis(),
        ac_online: ev.data.get("AcOnline")? == "true",
        pct,
    })
}

/// The stint the app finds at startup, on battery: since the last unplug, unless that was
/// before this boot (the laptop was unplugged while off), then since the boot.
pub fn current_stint(newest: Option<&PowerSourceEvent>, boot_ms: i64) -> BatteryStint {
    match newest {
        Some(e) if !e.ac_online && e.at_ms >= boot_ms => BatteryStint { since_ms: e.at_ms, since_pct: e.pct },
        _ => BatteryStint { since_ms: boot_ms, since_pct: None },
    }
}

/// The longest unplug-to-plug span in the log, newest event first.
pub fn longest_stint(events: &[PowerSourceEvent]) -> Option<Record> {
    let mut best: Option<Record> = None;
    let mut plugged_at: Option<i64> = None;
    for e in events {
        if e.ac_online {
            plugged_at = Some(e.at_ms);
        } else if let Some(end) = plugged_at.take() {
            raise(&mut best, ((end - e.at_ms) / 60_000) as f32, f32::MAX, e.at_ms);
        }
    }
    best
}

/// `records.json`; changes are kept in memory and written by `flush`.
pub struct RecordsLog {
    path: PathBuf,
    records: Records,
    dirty: bool,
}

impl RecordsLog {
    /// Never fails: a missing or corrupt file starts with no records (a corrupt one is kept as `.bak`).
    pub fn open(path: &Path) -> Self {
        let records = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
                let _ = std::fs::copy(path, path.with_extension("json.bak"));
                Records::default()
            }),
            Err(_) => Records::default(),
        };
        Self { path: path.to_owned(), records, dirty: false }
    }

    pub fn records(&self) -> &Records {
        &self.records
    }

    /// `f` returns whether it moved a record.
    pub fn update(&mut self, f: impl FnOnce(&mut Records) -> bool) {
        self.dirty |= f(&mut self.records);
    }

    /// Writes the records if they moved since the last write.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let json = serde_json::to_string(&self.records).map_err(std::io::Error::from)?;
        write_atomic(&self.path, json.as_bytes())?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcim::FanReading;
    use crate::sensors::CpuSnapshot;

    const MIN: i64 = 60_000;

    fn battery(state: PowerState, power_w: f32) -> BatterySnapshot {
        BatterySnapshot {
            percent: Some(60.0),
            state,
            power_w,
            remaining_mwh: 35_000,
            full_mwh: None,
            design_mwh: None,
            wear_pct: None,
            cycles: None,
            voltage_v: 12.0,
        }
    }

    fn telemetry(ts_ms: i64, cpu_c: f32, fan_rpm: u32) -> Telemetry {
        Telemetry {
            ts_ms,
            battery: Some(battery(PowerState::Discharging, -20.0)),
            cpu: CpuSnapshot { temp_c: Some(cpu_c), ..Default::default() },
            gpu: GpuSnapshot::Active { temp_c: 61, load_pct: 40 },
            fans: vec![FanReading { name: "CPU".into(), rpm: fan_rpm }, FanReading { name: "GPU".into(), rpm: 2100 }],
            mem: None,
            dimm_c: None,
            skin_c: None,
            power: None,
        }
    }

    #[test]
    fn first_reading_sets_every_record() {
        let mut r = Records::default();
        assert!(r.observe(&telemetry(1000, 72.0, 3400)));
        assert_eq!(r.cpu_temp_c, Some(Record { value: 72.0, at_ms: 1000 }));
        assert_eq!(r.gpu_temp_c, Some(Record { value: 61.0, at_ms: 1000 }));
        assert_eq!(r.fan_rpm, Some(Record { value: 3400.0, at_ms: 1000 }));
        assert_eq!(r.battery_draw_w, Some(Record { value: 20.0, at_ms: 1000 }));
    }

    #[test]
    fn a_record_moves_only_when_beaten() {
        let mut r = Records::default();
        r.observe(&telemetry(1000, 72.0, 3400));
        assert!(!r.observe(&telemetry(2000, 72.0, 3000)));
        assert!(r.observe(&telemetry(3000, 88.5, 3000)));
        assert_eq!(r.cpu_temp_c, Some(Record { value: 88.5, at_ms: 3000 }));
        assert_eq!(r.fan_rpm, Some(Record { value: 3400.0, at_ms: 1000 }));
    }

    #[test]
    fn glitched_readings_are_no_records() {
        let mut r = Records::default();
        r.observe(&telemetry(1000, 255.0, 65_535));
        assert_eq!((r.cpu_temp_c, r.fan_rpm), (None, None));
    }

    #[test]
    fn draw_counts_only_on_battery() {
        let mut r = Records::default();
        assert!(!r.observe_battery(&battery(PowerState::Charging, 60.0), 1000));
        assert!(r.observe_battery(&battery(PowerState::Discharging, -31.5), 2000));
        assert_eq!(r.battery_draw_w, Some(Record { value: 31.5, at_ms: 2000 }));
    }

    #[test]
    fn a_stint_counts_whole_minutes_from_its_start() {
        let mut r = Records::default();
        let stint = BatteryStint { since_ms: 10 * MIN, since_pct: Some(90.0) };
        assert!(!r.observe_stint(&stint, 10 * MIN + 30_000));
        assert!(r.observe_stint(&stint, 145 * MIN + 59_000));
        assert_eq!(r.battery_stint_min, Some(Record { value: 135.0, at_ms: 10 * MIN }));
    }

    fn event(time: &str, ac: bool, left: u32) -> String {
        format!(
            "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System>\
             <Provider Name='Microsoft-Windows-Kernel-Power'/><EventID>105</EventID>\
             <TimeCreated SystemTime='{time}'/></System><EventData>\
             <Data Name='AcOnline'>{ac}</Data><Data Name='RemainingCapacity'>{left}</Data>\
             <Data Name='FullChargeCapacity'>58709</Data></EventData></Event>"
        )
    }

    #[test]
    fn reads_the_power_source_event() {
        let e = power_source_event(&event("2026-10-28T07:43:58.5478242Z", false, 47124)).unwrap();
        assert!(!e.ac_online);
        assert_eq!(e.at_ms, 1_793_173_438_547);
        assert!((e.pct.unwrap() - 80.27).abs() < 0.01);
        assert!(power_source_event(&event("2026-10-28T07:43:58Z", true, 0)).unwrap().ac_online);
    }

    fn change(at_ms: i64, ac_online: bool) -> PowerSourceEvent {
        PowerSourceEvent { at_ms, ac_online, pct: Some(80.0) }
    }

    #[test]
    fn on_battery_since_the_last_unplug() {
        let stint = current_stint(Some(&change(50 * MIN, false)), 10 * MIN);
        assert_eq!(stint, BatteryStint { since_ms: 50 * MIN, since_pct: Some(80.0) });
    }

    #[test]
    fn unplugged_while_off_counts_from_the_boot() {
        let stint = BatteryStint { since_ms: 10 * MIN, since_pct: None };
        assert_eq!(current_stint(Some(&change(5 * MIN, false)), 10 * MIN), stint);
        // The log's last word is a plug in: the unplug happened while off.
        assert_eq!(current_stint(Some(&change(50 * MIN, true)), 10 * MIN), stint);
        assert_eq!(current_stint(None, 10 * MIN), stint);
    }

    #[test]
    fn longest_stint_pairs_each_unplug_with_the_next_plug() {
        // Newest first: 0–90 min unplugged, plugged, 100–130 unplugged, plugged, then unplugged now.
        let events = [change(200 * MIN, false), change(130 * MIN, true), change(100 * MIN, false), change(90 * MIN, true), change(0, false)];
        assert_eq!(longest_stint(&events), Some(Record { value: 90.0, at_ms: 0 }));
        assert_eq!(longest_stint(&[change(5 * MIN, false)]), None);
    }

    #[test]
    fn records_persist_on_flush_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.json");
        let mut log = RecordsLog::open(&path);
        log.update(|r| r.observe(&telemetry(1000, 72.0, 3400)));
        assert_eq!(RecordsLog::open(&path).records(), &Records::default());
        log.flush().unwrap();
        assert_eq!(RecordsLog::open(&path).records(), log.records());
    }

    #[test]
    fn corrupt_records_start_empty_and_are_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.json");
        std::fs::write(&path, "xx").unwrap();
        assert_eq!(RecordsLog::open(&path).records(), &Records::default());
        assert_eq!(std::fs::read_to_string(dir.path().join("records.json.bak")).unwrap(), "xx");
    }
}
