//! CPU clock and performance cap through Windows' `Processor Information` performance counters.
//! `Processor Frequency` there moves with the P-state, so the nominal clock comes from the registry.

use mystation_core::cpu_perf::{clock_mhz, limit_pct};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCloseQuery,
    PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::core::{PCWSTR, w};

/// Open for the poller's lifetime: `% Processor Performance` is a rate, so each reading needs the one before.
pub struct CpuPerfReader {
    query: PDH_HQUERY,
    base_mhz: u32,
    perf: PDH_HCOUNTER,
    limit: PDH_HCOUNTER,
}

impl CpuPerfReader {
    /// `None` when the counters aren't there.
    pub fn new() -> Option<Self> {
        let base_mhz = super::registry::hklm_dword(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "~MHz")?;
        let mut query = PDH_HQUERY::default();
        // SAFETY: the query is closed on failure here or in `Drop`; out-params are live locals.
        unsafe {
            if PdhOpenQueryW(None, 0, &mut query) != ERROR_SUCCESS.0 {
                return None;
            }
            // English paths, so they resolve on any display language.
            let add = |path: PCWSTR| {
                let mut counter = PDH_HCOUNTER::default();
                (PdhAddEnglishCounterW(query, path, 0, &mut counter) == ERROR_SUCCESS.0).then_some(counter)
            };
            let counters = (
                add(w!(r"\Processor Information(_Total)\% Processor Performance")),
                add(w!(r"\Processor Information(_Total)\% Performance Limit")),
            );
            match counters {
                (Some(perf), Some(limit)) if PdhCollectQueryData(query) == ERROR_SUCCESS.0 => {
                    Some(Self { query, base_mhz, perf, limit })
                }
                _ => {
                    let _ = PdhCloseQuery(query);
                    None
                }
            }
        }
    }

    /// Average clock in MHz and the performance cap since the previous call.
    pub fn read(&mut self) -> (Option<u32>, Option<f32>) {
        // SAFETY: the query is open until `Drop`.
        if unsafe { PdhCollectQueryData(self.query) } != ERROR_SUCCESS.0 {
            return (None, None);
        }
        let clock = value(self.perf).and_then(|perf| clock_mhz(self.base_mhz.into(), perf));
        (clock, value(self.limit).and_then(limit_pct))
    }
}

fn value(counter: PDH_HCOUNTER) -> Option<f64> {
    let mut value = PDH_FMT_COUNTERVALUE::default();
    // SAFETY: the counter belongs to an open query; `value` is a live local.
    unsafe {
        (PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, None, &mut value) == ERROR_SUCCESS.0
            && value.CStatus == 0)
            .then_some(value.Anonymous.doubleValue)
    }
}

impl Drop for CpuPerfReader {
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
    fn reads_clock_and_limit() {
        let mut reader = CpuPerfReader::new().expect("Processor Information counters");
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let (clock, limit) = reader.read();
        println!("CPU clock: {clock:?} MHz, limit: {limit:?}");
        assert!(clock.is_some_and(|mhz| (200..8000).contains(&mhz)));
    }
}
