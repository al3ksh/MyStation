//! Per-instance performance counters: the load of each logical processor and each physical
//! disk's activity, keyed by the instance names Windows gives them.

use std::collections::BTreeMap;

use serde::Serialize;

/// The number an instance name starts with: `"3"` → 3, `"0 C:"` → 0; `None` for `"_Total"`.
pub fn instance_index(name: &str) -> Option<u32> {
    name.split_whitespace().next()?.parse().ok()
}

/// `% Processor Time` per logical processor, in processor order, without `_Total`.
pub fn core_loads<'a>(samples: impl IntoIterator<Item = (&'a str, f64)>) -> Vec<f32> {
    let by_index: BTreeMap<u32, f32> = samples
        .into_iter()
        .filter_map(|(name, v)| Some((instance_index(name)?, v.clamp(0.0, 100.0) as f32)))
        .collect();
    by_index.into_values().collect()
}

/// One physical disk's counters, read together.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DiskRates {
    pub idle_pct: f64,
    pub read_bps: f64,
    pub write_bps: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveSnapshot {
    /// `\\.\PhysicalDrive<index>`.
    pub index: u32,
    pub model: Option<String>,
    pub temp_c: Option<i32>,
    /// Task Manager's "Active time": the share of time the disk was busy.
    pub active_pct: Option<f32>,
    pub read_bps: Option<f64>,
    pub write_bps: Option<f64>,
}

/// What a drive reports about itself, read less often than the counters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DriveInfo {
    pub index: u32,
    pub model: Option<String>,
    pub temp_c: Option<i32>,
}

/// The drives with their counters; a disk the counters miss keeps its name and temperature.
pub fn drive_snapshots(info: &[DriveInfo], rates: &BTreeMap<u32, DiskRates>) -> Vec<DriveSnapshot> {
    info.iter()
        .map(|d| {
            let r = rates.get(&d.index);
            DriveSnapshot {
                index: d.index,
                model: d.model.clone(),
                temp_c: d.temp_c,
                active_pct: r.map(|r| (100.0 - r.idle_pct).clamp(0.0, 100.0) as f32),
                read_bps: r.map(|r| r.read_bps.max(0.0)),
                write_bps: r.map(|r| r.write_bps.max(0.0)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_names_give_their_number() {
        assert_eq!(instance_index("3"), Some(3));
        assert_eq!(instance_index("0 C:"), Some(0));
        assert_eq!(instance_index("1 D: E:"), Some(1));
        assert_eq!(instance_index("_Total"), None);
        assert_eq!(instance_index(""), None);
    }

    #[test]
    fn core_loads_follow_processor_order_without_the_total() {
        let samples = [("10", 5.0), ("_Total", 30.0), ("2", 101.5), ("0", 12.5), ("1", -0.2)];
        assert_eq!(core_loads(samples), vec![12.5, 0.0, 100.0, 5.0]);
    }

    #[test]
    fn drives_pair_with_their_counters_by_index() {
        let info = [
            DriveInfo { index: 0, model: Some("Samsung PM9A1".into()), temp_c: Some(41) },
            DriveInfo { index: 1, model: None, temp_c: None },
        ];
        let rates = BTreeMap::from([(0, DiskRates { idle_pct: 92.5, read_bps: 1.5e6, write_bps: 2e5 })]);
        let d = drive_snapshots(&info, &rates);
        assert_eq!(d[0].active_pct, Some(7.5));
        assert_eq!(d[0].read_bps, Some(1.5e6));
        assert_eq!(d[0].temp_c, Some(41));
        assert_eq!(d[1], DriveSnapshot { index: 1, model: None, temp_c: None, active_pct: None, read_bps: None, write_bps: None });
    }

    #[test]
    fn idle_time_past_100_is_no_negative_activity() {
        let info = [DriveInfo { index: 0, ..Default::default() }];
        let rates = BTreeMap::from([(0, DiskRates { idle_pct: 100.4, read_bps: 0.0, write_bps: 0.0 })]);
        assert_eq!(drive_snapshots(&info, &rates)[0].active_pct, Some(0.0));
    }
}
