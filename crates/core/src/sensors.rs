//! Telemetry types sent to the UI.

use serde::Serialize;

pub use crate::battery::{BatteryRaw, BatterySnapshot, PowerState, battery_from_raw};
pub use crate::cpu_load::{CpuTimes, load_between};
pub use crate::dcim::{DcimReadings, DcimRow, FanReading, parse_dcim};
pub use crate::power::{PowerFlow, PowerWindow, power_flow};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuSnapshot {
    pub load_pct: Option<f32>,
    pub temp_c: Option<f32>,
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
    Active { temp_c: u32, load_pct: u32 },
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
            serde_json::to_string(&GpuSnapshot::Active { temp_c: 61, load_pct: 40 }).unwrap(),
            r#"{"state":"active","tempC":61,"loadPct":40}"#
        );
    }
}
