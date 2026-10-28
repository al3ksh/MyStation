//! CPU clock and the firmware's performance cap, from Windows' `Processor Information` counters.

/// Actual clock: `% Processor Performance` scales the nominal clock, past 100 in turbo.
pub fn clock_mhz(base_mhz: f64, perf_pct: f64) -> Option<u32> {
    let mhz = base_mhz * perf_pct / 100.0;
    (mhz.is_finite() && mhz > 0.0).then(|| mhz.round() as u32)
}

/// The cap while the firmware holds the CPU back (`% Performance Limit` under 100); `None` at full.
pub fn limit_pct(limit_pct: f64) -> Option<f32> {
    let pct = limit_pct.round();
    (pct > 0.0 && pct < 100.0).then_some(pct as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_scales_the_base_frequency() {
        assert_eq!(clock_mhz(2496.0, 79.0), Some(1972));
        assert_eq!(clock_mhz(2496.0, 184.5), Some(4605));
    }

    #[test]
    fn clock_needs_both_readings() {
        assert_eq!(clock_mhz(0.0, 79.0), None);
        assert_eq!(clock_mhz(2496.0, 0.0), None);
        assert_eq!(clock_mhz(f64::NAN, 79.0), None);
    }

    #[test]
    fn full_performance_is_no_limit() {
        assert_eq!(limit_pct(100.0), None);
        assert_eq!(limit_pct(99.7), None);
    }

    #[test]
    fn a_cap_reads_as_its_percent() {
        assert_eq!(limit_pct(72.0), Some(72.0));
        assert_eq!(limit_pct(45.4), Some(45.0));
    }

    #[test]
    fn a_missing_cap_is_no_limit() {
        assert_eq!(limit_pct(0.0), None);
        assert_eq!(limit_pct(f64::NAN), None);
    }
}
