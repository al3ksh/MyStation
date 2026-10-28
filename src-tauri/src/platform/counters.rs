//! Per-instance performance counters: each logical processor's load and each physical disk's
//! activity and throughput. All of them are rates, so the query stays open between readings.

use std::collections::BTreeMap;

use mystation_core::counters::{DiskRates, core_loads, instance_index};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};
use windows::core::{PCWSTR, w};

pub struct CountersReader {
    query: PDH_HQUERY,
    cores: PDH_HCOUNTER,
    disk_idle: PDH_HCOUNTER,
    disk_read: PDH_HCOUNTER,
    disk_write: PDH_HCOUNTER,
}

/// Per-core loads and per-disk rates since the previous reading.
#[derive(Default)]
pub struct CounterSample {
    pub core_loads: Vec<f32>,
    pub disks: BTreeMap<u32, DiskRates>,
}

impl CountersReader {
    /// `None` when the counters aren't there.
    pub fn new() -> Option<Self> {
        let mut query = PDH_HQUERY::default();
        // SAFETY: the query is closed on failure here or in `Drop`; out-params are live locals.
        unsafe {
            if PdhOpenQueryW(None, 0, &mut query) != ERROR_SUCCESS.0 {
                return None;
            }
            let add = |path: PCWSTR| {
                let mut counter = PDH_HCOUNTER::default();
                (PdhAddEnglishCounterW(query, path, 0, &mut counter) == ERROR_SUCCESS.0).then_some(counter)
            };
            let counters = (
                add(w!(r"\Processor(*)\% Processor Time")),
                add(w!(r"\PhysicalDisk(*)\% Idle Time")),
                add(w!(r"\PhysicalDisk(*)\Disk Read Bytes/sec")),
                add(w!(r"\PhysicalDisk(*)\Disk Write Bytes/sec")),
            );
            match counters {
                (Some(cores), Some(disk_idle), Some(disk_read), Some(disk_write))
                    if PdhCollectQueryData(query) == ERROR_SUCCESS.0 =>
                {
                    Some(Self { query, cores, disk_idle, disk_read, disk_write })
                }
                _ => {
                    let _ = PdhCloseQuery(query);
                    None
                }
            }
        }
    }

    pub fn read(&mut self) -> CounterSample {
        // SAFETY: the query is open until `Drop`.
        if unsafe { PdhCollectQueryData(self.query) } != ERROR_SUCCESS.0 {
            return CounterSample::default();
        }
        let cores = array(self.cores);
        let mut disks: BTreeMap<u32, DiskRates> = BTreeMap::new();
        for (name, v) in array(self.disk_idle) {
            if let Some(i) = instance_index(&name) {
                disks.entry(i).or_default().idle_pct = v;
            }
        }
        for (name, v) in array(self.disk_read) {
            if let Some(d) = instance_index(&name).and_then(|i| disks.get_mut(&i)) {
                d.read_bps = v;
            }
        }
        for (name, v) in array(self.disk_write) {
            if let Some(d) = instance_index(&name).and_then(|i| disks.get_mut(&i)) {
                d.write_bps = v;
            }
        }
        CounterSample { core_loads: core_loads(cores.iter().map(|(n, v)| (n.as_str(), *v))), disks }
    }
}

/// Every instance of a wildcard counter with its value; instances without a valid value are left out.
fn array(counter: PDH_HCOUNTER) -> Vec<(String, f64)> {
    let (mut size, mut count) = (0u32, 0u32);
    // SAFETY: the first call only asks for the buffer size; the second gets a buffer of that many
    // bytes, aligned for the items, which also holds the names they point to.
    unsafe {
        if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None) != PDH_MORE_DATA {
            return Vec::new();
        }
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let items = buf.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items))
            != ERROR_SUCCESS.0
        {
            return Vec::new();
        }
        std::slice::from_raw_parts(items, count as usize)
            .iter()
            .filter(|item| item.FmtValue.CStatus == 0)
            .filter_map(|item| Some((item.szName.to_string().ok()?, item.FmtValue.Anonymous.doubleValue)))
            .collect()
    }
}

impl Drop for CountersReader {
    fn drop(&mut self) {
        // SAFETY: opened in `new`, closed once here.
        unsafe {
            let _ = PdhCloseQuery(self.query);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "reads this machine's counters"]
    fn reads_cores_and_disks() {
        let mut reader = CountersReader::new().expect("Processor and PhysicalDisk counters");
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let sample = reader.read();
        println!("cores: {:?}\ndisks: {:?}", sample.core_loads, sample.disks);
        assert_eq!(sample.core_loads.len(), std::thread::available_parallelism().unwrap().get());
        assert!(!sample.disks.is_empty());
    }
}
