//! CPU package power from Intel RAPL, through Windows' `Energy Meter` performance counters.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCloseQuery,
    PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::core::w;

/// Open for the poller's lifetime: the counter is a rate, so each reading needs the one before.
pub struct CpuPowerReader {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

impl CpuPowerReader {
    /// `None` when the counters aren't there (no RAPL, or an older Windows).
    pub fn new() -> Option<Self> {
        let mut query = PDH_HQUERY::default();
        let mut counter = PDH_HCOUNTER::default();
        // SAFETY: the query is closed on failure here or in `Drop`; out-params are live locals.
        unsafe {
            if PdhOpenQueryW(None, 0, &mut query) != ERROR_SUCCESS.0 {
                return None;
            }
            // English path, so it resolves on any display language.
            if PdhAddEnglishCounterW(query, w!(r"\Energy Meter(RAPL_Package0_PKG)\Power"), 0, &mut counter)
                != ERROR_SUCCESS.0
                || PdhCollectQueryData(query) != ERROR_SUCCESS.0
            {
                let _ = PdhCloseQuery(query);
                return None;
            }
        }
        Some(Self { query, counter })
    }

    /// Average package power in watts since the previous call.
    pub fn read(&mut self) -> Option<f32> {
        let mut value = PDH_FMT_COUNTERVALUE::default();
        // SAFETY: query and counter are open until `Drop`; `value` is a live local.
        unsafe {
            if PdhCollectQueryData(self.query) != ERROR_SUCCESS.0
                || PdhGetFormattedCounterValue(self.counter, PDH_FMT_DOUBLE, None, &mut value) != ERROR_SUCCESS.0
                || value.CStatus != 0
            {
                return None;
            }
            // The counter is in milliwatts.
            Some((value.Anonymous.doubleValue / 1000.0) as f32)
        }
    }
}

impl Drop for CpuPowerReader {
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
    fn reads_package_power() {
        let mut reader = CpuPowerReader::new().expect("Energy Meter counters");
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let w = reader.read().expect("a reading");
        println!("CPU package: {w} W");
        assert!((0.1..200.0).contains(&w));
    }
}
