//! 30-minute telemetry ring buffer and the daily battery health log.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::config::write_atomic;
use crate::sensors::{GpuSnapshot, Telemetry};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistorySample {
    pub ts_ms: i64,
    pub cpu_temp: Option<f32>,
    pub cpu_load: Option<f32>,
    pub gpu_temp: Option<u32>,
    pub gpu_load: Option<u32>,
    pub battery_pct: Option<f32>,
    pub battery_w: Option<f32>,
}

impl From<&Telemetry> for HistorySample {
    fn from(t: &Telemetry) -> Self {
        let battery = t.battery.as_ref();
        Self {
            ts_ms: t.ts_ms,
            cpu_temp: t.cpu.temp_c,
            cpu_load: t.cpu.load_pct,
            gpu_temp: match t.gpu {
                GpuSnapshot::Active { temp_c, .. } => Some(temp_c),
                _ => None,
            },
            gpu_load: match t.gpu {
                GpuSnapshot::Active { load_pct, .. } => Some(load_pct),
                _ => None,
            },
            battery_pct: battery.and_then(|b| b.percent),
            battery_w: battery.map(|b| b.power_w),
        }
    }
}

pub struct History {
    samples: VecDeque<HistorySample>,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    /// 30 minutes at one sample per second.
    pub const CAPACITY: usize = 1800;

    pub fn new() -> Self {
        Self { samples: VecDeque::with_capacity(Self::CAPACITY) }
    }

    pub fn push(&mut self, s: HistorySample) {
        if self.samples.len() == Self::CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(s);
    }

    /// Samples from the last `minutes` minutes, oldest first.
    pub fn range(&self, minutes: u32, now_ms: i64) -> Vec<HistorySample> {
        let from = now_ms - i64::from(minutes) * 60_000;
        self.samples.iter().filter(|s| s.ts_ms >= from).cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HealthEntry {
    pub date: NaiveDate,
    pub full_mwh: u32,
    pub design_mwh: u32,
    pub cycles: Option<u32>,
}

pub struct HealthLog {
    path: PathBuf,
    entries: Vec<HealthEntry>,
}

impl HealthLog {
    /// Never fails: a missing or corrupt file starts an empty log (a corrupt one is kept as `.bak`).
    pub fn open(path: &Path) -> Self {
        let entries = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| {
                let _ = std::fs::copy(path, path.with_extension("json.bak"));
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        Self { path: path.to_owned(), entries }
    }

    pub fn entries(&self) -> &[HealthEntry] {
        &self.entries
    }

    /// Records at most one entry per day; returns `false` when the day is already logged.
    pub fn record(&mut self, entry: HealthEntry) -> std::io::Result<bool> {
        if self.entries.iter().any(|e| e.date == entry.date) {
            return Ok(false);
        }
        self.entries.push(entry);
        if let Err(e) = serde_json::to_string(&self.entries)
            .map_err(std::io::Error::from)
            .and_then(|json| write_atomic(&self.path, json.as_bytes()))
        {
            self.entries.pop(); // not persisted: let the next poll retry today's entry
            return Err(e);
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::{BatterySnapshot, CpuSnapshot, PowerState};

    fn sample(ts_ms: i64) -> HistorySample {
        HistorySample { ts_ms, cpu_temp: None, cpu_load: None, gpu_temp: None, gpu_load: None, battery_pct: None, battery_w: None }
    }

    #[test]
    fn sample_from_telemetry() {
        let t = Telemetry {
            ts_ms: 7,
            battery: Some(BatterySnapshot {
                percent: Some(80.0),
                state: PowerState::Discharging,
                power_w: -9.5,
                remaining_mwh: 1,
                full_mwh: None,
                design_mwh: None,
                wear_pct: None,
                cycles: None,
                voltage_v: 12.0,
            }),
            cpu: CpuSnapshot { load_pct: Some(12.0), temp_c: Some(55.0), ..Default::default() },
            gpu: GpuSnapshot::active(60, 3),
            fans: vec![],
            mem: None,
            dimm_c: None,
            skin_c: None,
            drives: vec![],
            power: None,
        };
        assert_eq!(
            HistorySample::from(&t),
            HistorySample {
                ts_ms: 7,
                cpu_temp: Some(55.0),
                cpu_load: Some(12.0),
                gpu_temp: Some(60),
                gpu_load: Some(3),
                battery_pct: Some(80.0),
                battery_w: Some(-9.5),
            }
        );
        let asleep = Telemetry { gpu: GpuSnapshot::Asleep, battery: None, ..t };
        let s = HistorySample::from(&asleep);
        assert_eq!((s.gpu_temp, s.gpu_load, s.battery_pct, s.battery_w), (None, None, None, None));
    }

    #[test]
    fn ring_drops_oldest() {
        let mut h = History::new();
        for ts in 0..=1800 {
            h.push(sample(ts));
        }
        assert_eq!(h.len(), 1800);
        assert_eq!(h.range(30, 1800)[0].ts_ms, 1);
    }

    #[test]
    fn range_filters_by_time() {
        let mut h = History::new();
        let now = 20 * 60 * 1000;
        for i in 0..=1200 {
            h.push(sample(i * 1000));
        }
        let r = h.range(5, now);
        assert!(r.len() == 300 || r.len() == 301, "len {}", r.len());
        assert!(r.iter().all(|s| s.ts_ms >= now - 300_000));
        assert_eq!(r.last().unwrap().ts_ms, now);
    }

    fn entry(day: u32, full: u32) -> HealthEntry {
        HealthEntry { date: NaiveDate::from_ymd_opt(2026, 10, day).unwrap(), full_mwh: full, design_mwh: 67914, cycles: None }
    }

    #[test]
    fn health_one_per_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = HealthLog::open(&dir.path().join("health.json"));
        assert!(log.record(entry(9, 58709)).unwrap());
        assert!(!log.record(entry(9, 50000)).unwrap());
        assert_eq!(log.entries(), &[entry(9, 58709)]);
        assert!(log.record(entry(10, 58700)).unwrap());
        assert_eq!(log.entries().len(), 2);
    }

    #[test]
    fn health_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("health.json");
        HealthLog::open(&path).record(entry(9, 58709)).unwrap();
        assert_eq!(HealthLog::open(&path).entries(), &[entry(9, 58709)]);
    }

    #[test]
    fn health_corrupt_file_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("health.json");
        std::fs::write(&path, "xx").unwrap();
        assert!(HealthLog::open(&path).entries().is_empty());
    }

    #[test]
    fn health_corrupt_file_is_backed_up_before_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("health.json");
        std::fs::write(&path, "xx").unwrap();
        HealthLog::open(&path).record(entry(9, 58709)).unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("health.json.bak")).unwrap(), "xx");
    }
}
