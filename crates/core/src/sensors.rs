//! Telemetry types sent to the UI.

use serde::Serialize;

pub use crate::counters::DriveSnapshot;
pub use crate::battery::{BatteryRaw, BatterySnapshot, PowerState, battery_from_raw};
pub use crate::cpu_load::{CpuTimes, load_between};
pub use crate::dcim::{DcimReadings, DcimRow, FanReading, parse_dcim};
pub use crate::power::{PowerFlow, PowerWindow, power_flow};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuSnapshot {
    pub load_pct: Option<f32>,
    pub temp_c: Option<f32>,
    pub clock_mhz: Option<u32>,
    /// The firmware's performance cap; `None` while the CPU runs uncapped.
    pub limit_pct: Option<f32>,
    /// Each logical processor's load, in processor order; empty when the counters aren't there.
    pub core_loads: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemSnapshot {
    pub used_mb: u32,
    pub total_mb: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GpuSnapshot {
    Asleep,
    Unavailable,
    Active {
        temp_c: u32,
        load_pct: u32,
        clock_mhz: Option<u32>,
        mem_clock_mhz: Option<u32>,
        vram_used_mb: Option<u32>,
        vram_total_mb: Option<u32>,
    },
}

impl GpuSnapshot {
    /// An active reading with only the temperature and load known.
    pub fn active(temp_c: u32, load_pct: u32) -> Self {
        Self::Active { temp_c, load_pct, clock_mhz: None, mem_clock_mhz: None, vram_used_mb: None, vram_total_mb: None }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Telemetry {
    pub ts_ms: i64,
    pub battery: Option<BatterySnapshot>,
    pub cpu: CpuSnapshot,
    pub gpu: GpuSnapshot,
    pub fans: Vec<FanReading>,
    pub mem: Option<MemSnapshot>,
    pub dimm_c: Option<f32>,
    pub skin_c: Option<f32>,
    pub drives: Vec<DriveSnapshot>,
    /// `None` without a battery reading.
    pub power: Option<PowerFlow>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_serializes_for_ui() {
        assert_eq!(serde_json::to_string(&GpuSnapshot::Asleep).unwrap(), r#"{"state":"asleep"}"#);
        assert_eq!(
            serde_json::to_string(&GpuSnapshot::active(61, 40)).unwrap(),
            r#"{"state":"active","tempC":61,"loadPct":40,"clockMhz":null,"memClockMhz":null,"vramUsedMb":null,"vramTotalMb":null}"#
        );
    }
}
