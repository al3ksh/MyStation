//! The current time on battery, from Windows' power source changes (Kernel-Power event 105).

use serde::Serialize;

use crate::boot::parse_event;

pub const POWER_PROVIDER: &str = "Microsoft-Windows-Kernel-Power";
/// Logged on every plug in and unplug, with the charge at that moment.
pub const POWER_SOURCE_EVENT: u32 = 105;

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

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60_000;

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
}
