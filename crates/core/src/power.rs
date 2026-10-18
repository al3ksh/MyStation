//! Where the power goes: the battery rate split into CPU, NVIDIA GPU and the rest.

use serde::Serialize;

use crate::battery::{BatterySnapshot, PowerState};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "source", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PowerFlow {
    /// Everything the laptop draws comes out of the battery, so the parts add up to the total.
    Battery { total_w: f32, cpu_w: Option<f32>, gpu_w: Option<f32>, rest_w: Option<f32> },
    /// The adapter's own draw isn't reported, so there is no total to split.
    Adapter { cpu_w: Option<f32>, gpu_w: Option<f32>, into_battery_w: f32, adapter_w: Option<u32> },
}

pub fn power_flow(
    battery: &BatterySnapshot,
    cpu_w: Option<f32>,
    gpu_w: Option<f32>,
    adapter_w: Option<u32>,
) -> PowerFlow {
    match battery.state {
        PowerState::Discharging => {
            let total_w = -battery.power_w;
            let rest_w = cpu_w.map(|cpu| round1((total_w - cpu - gpu_w.unwrap_or(0.0)).max(0.0)));
            PowerFlow::Battery { total_w, cpu_w, gpu_w, rest_w }
        }
        PowerState::Charging | PowerState::Bypass => {
            PowerFlow::Adapter { cpu_w, gpu_w, into_battery_w: battery.power_w, adapter_w }
        }
    }
}

fn round1(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery(state: PowerState, power_w: f32) -> BatterySnapshot {
        BatterySnapshot {
            percent: Some(80.0),
            state,
            power_w,
            remaining_mwh: 50_000,
            full_mwh: Some(60_000),
            design_mwh: Some(68_000),
            wear_pct: None,
            cycles: None,
            voltage_v: 12.0,
        }
    }

    #[test]
    fn on_battery_the_rest_is_what_cpu_and_gpu_leave() {
        let flow = power_flow(&battery(PowerState::Discharging, -18.0), Some(6.5), Some(0.0), None);
        assert_eq!(
            flow,
            PowerFlow::Battery { total_w: 18.0, cpu_w: Some(6.5), gpu_w: Some(0.0), rest_w: Some(11.5) }
        );
    }

    #[test]
    fn on_battery_without_a_gpu_reading_the_rest_includes_it() {
        let flow = power_flow(&battery(PowerState::Discharging, -10.0), Some(4.0), None, None);
        assert_eq!(flow, PowerFlow::Battery { total_w: 10.0, cpu_w: Some(4.0), gpu_w: None, rest_w: Some(6.0) });
    }

    #[test]
    fn on_battery_without_a_cpu_reading_there_is_no_rest() {
        let flow = power_flow(&battery(PowerState::Discharging, -10.0), None, Some(3.0), None);
        assert_eq!(flow, PowerFlow::Battery { total_w: 10.0, cpu_w: None, gpu_w: Some(3.0), rest_w: None });
    }

    #[test]
    fn the_rest_never_goes_negative() {
        // The battery rate is averaged by the firmware and lags a sudden load.
        let flow = power_flow(&battery(PowerState::Discharging, -8.0), Some(9.0), Some(1.0), None);
        assert_eq!(flow, PowerFlow::Battery { total_w: 8.0, cpu_w: Some(9.0), gpu_w: Some(1.0), rest_w: Some(0.0) });
    }

    #[test]
    fn on_ac_shows_the_parts_without_a_total() {
        let flow = power_flow(&battery(PowerState::Charging, 33.0), Some(25.0), Some(12.0), Some(130));
        assert_eq!(
            flow,
            PowerFlow::Adapter { cpu_w: Some(25.0), gpu_w: Some(12.0), into_battery_w: 33.0, adapter_w: Some(130) }
        );
    }

    #[test]
    fn at_the_charge_limit_nothing_goes_into_the_battery() {
        let flow = power_flow(&battery(PowerState::Bypass, 0.0), Some(5.0), Some(0.0), None);
        assert_eq!(
            flow,
            PowerFlow::Adapter { cpu_w: Some(5.0), gpu_w: Some(0.0), into_battery_w: 0.0, adapter_w: None }
        );
    }

    #[test]
    fn serializes_for_ui() {
        let flow = PowerFlow::Battery { total_w: 18.0, cpu_w: Some(6.5), gpu_w: None, rest_w: Some(11.5) };
        assert_eq!(
            serde_json::to_string(&flow).unwrap(),
            r#"{"source":"battery","totalW":18.0,"cpuW":6.5,"gpuW":null,"restW":11.5}"#
        );
    }
}
