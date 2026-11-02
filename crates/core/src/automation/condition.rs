//! What a routine waits for. Each condition reads as true, false or unknown: unknown is an input
//! not read yet, and it changes nothing.

use chrono::{Datelike, Duration, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Condition {
    Power { plugged: bool },
    /// Days: 0 = Monday … 6 = Sunday; times: local `HH:MM`. Without `until`, the moment `time`;
    /// with it, the stretch from `time` to `until`, into the next day when `until` is earlier.
    Time {
        days: Vec<u8>,
        time: String,
        #[serde(default)]
        until: Option<String>,
    },
    /// `connected: false` holds while the laptop is on any other network or none.
    Wifi { ssid: String, connected: bool },
    /// Any display besides the built-in one, a dock's included.
    Display { connected: bool },
    Bluetooth { device: String, connected: bool },
    BatteryBelow { percent: u8 },
    /// On AC without a break for this many days.
    LongAc { days: u32 },
}

/// Everything conditions read, as of this tick. `None` for an input not read yet.
#[derive(Clone, Copy, Debug)]
pub struct Readings<'a> {
    pub now: NaiveDateTime,
    pub on_ac: bool,
    /// Start of the current continuous AC period.
    pub ac_since: Option<NaiveDateTime>,
    pub battery_pct: Option<f32>,
    /// `Some(None)`: not connected to Wi-Fi.
    pub wifi: Option<&'a Option<String>>,
    /// External displays.
    pub displays: Option<u32>,
    /// Names of the connected Bluetooth devices.
    pub bluetooth: Option<&'a [String]>,
}

pub(crate) fn parse_time(time: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(time, "%H:%M").ok()
}

impl Condition {
    /// Whether it holds now; `None` when its input is unknown. A moment ("At") always holds here:
    /// the engine decides when it is due.
    pub fn status(&self, r: &Readings) -> Option<bool> {
        match self {
            Condition::Power { plugged } => Some(r.on_ac == *plugged),
            Condition::Time { until: None, .. } => Some(true),
            Condition::Time { days, time, until: Some(until) } => Some(in_window(days, time, until, r.now)),
            Condition::Wifi { ssid, connected } => r.wifi.map(|w| (w.as_deref() == Some(ssid.as_str())) == *connected),
            Condition::Display { connected } => r.displays.map(|n| (n > 0) == *connected),
            Condition::Bluetooth { device, connected } => r.bluetooth.map(|b| b.contains(device) == *connected),
            // Whole percents, so a reading of 19.9 is below 20 and 20.4 is not.
            Condition::BatteryBelow { percent } => r.battery_pct.map(|p| (p.clamp(0.0, 100.0) as u8) < *percent),
            Condition::LongAc { days } => {
                Some(r.on_ac && r.ac_since.is_some_and(|since| r.now - since >= Duration::days((*days).into())))
            }
        }
    }

    /// "At" a time: something that happens, not a stretch the routine runs for.
    pub fn is_moment(&self) -> bool {
        matches!(self, Condition::Time { until: None, .. })
    }

    /// The tile it comes from in the editor, for messages.
    pub fn label(&self) -> &'static str {
        match self {
            Condition::Power { .. } => "Power",
            Condition::Time { .. } => "Time",
            Condition::Wifi { .. } => "Wi-Fi",
            Condition::Display { .. } => "Display",
            Condition::Bluetooth { .. } => "Bluetooth",
            Condition::BatteryBelow { .. } => "Battery",
            Condition::LongAc { .. } => "Days on AC",
        }
    }
}

/// All hold: one that doesn't wins over one that is unknown.
pub fn all_hold(conditions: &[Condition], r: &Readings) -> Option<bool> {
    let mut known = true;
    for c in conditions {
        match c.status(r) {
            Some(false) => return Some(false),
            None => known = false,
            Some(true) => {}
        }
    }
    known.then_some(true)
}

/// The latest moment a time passed at or before `now`, looking back a week.
pub(crate) fn last_moment(days: &[u8], time: &str, now: NaiveDateTime) -> Option<NaiveDateTime> {
    let time = parse_time(time)?;
    (0..=7)
        .map(|back| (now.date() - Duration::days(back)).and_time(time))
        .find(|m| *m <= now && days.contains(&(m.weekday().num_days_from_monday() as u8)))
}

/// Whether `now` falls in a stretch that starts on one of `days` at `from` and lasts until `until`.
pub(crate) fn in_window(days: &[u8], from: &str, until: &str, now: NaiveDateTime) -> bool {
    let (Some(from), Some(until)) = (parse_time(from), parse_time(until)) else { return false };
    let mut length = until - from;
    if length <= Duration::zero() {
        length += Duration::days(1);
    }
    // A stretch that started yesterday may still be going.
    (0..=1).any(|back| {
        let start = (now.date() - Duration::days(back)).and_time(from);
        days.contains(&(start.weekday().num_days_from_monday() as u8)) && start <= now && now < start + length
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    // 2026-10-05 is a Monday.
    fn at(day: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn readings(now: NaiveDateTime) -> Readings<'static> {
        Readings { now, on_ac: true, ac_since: None, battery_pct: None, wifi: None, displays: None, bluetooth: None }
    }

    fn wifi(ssid: &str, connected: bool) -> Condition {
        Condition::Wifi { ssid: ssid.into(), connected }
    }

    #[test]
    fn power_follows_the_source() {
        let r = Readings { on_ac: false, ..readings(at(5, 9, 0)) };
        assert_eq!(Condition::Power { plugged: false }.status(&r), Some(true));
        assert_eq!(Condition::Power { plugged: true }.status(&r), Some(false));
    }

    #[test]
    fn wifi_is_unknown_until_read() {
        assert_eq!(wifi("Office", true).status(&readings(at(5, 9, 0))), None);
        assert_eq!(wifi("Office", false).status(&readings(at(5, 9, 0))), None);
    }

    #[test]
    fn wifi_connected_and_not() {
        let office = Some("Office".to_string());
        let none = None;
        let on = Readings { wifi: Some(&office), ..readings(at(5, 9, 0)) };
        let off = Readings { wifi: Some(&none), ..readings(at(5, 9, 0)) };
        assert_eq!(wifi("Office", true).status(&on), Some(true));
        assert_eq!(wifi("Office", false).status(&on), Some(false));
        assert_eq!(wifi("Office", true).status(&off), Some(false));
        assert_eq!(wifi("Office", false).status(&off), Some(true));
        assert_eq!(wifi("Home", false).status(&on), Some(true));
    }

    #[test]
    fn display_counts_external_ones() {
        let one = Readings { displays: Some(1), ..readings(at(5, 9, 0)) };
        let zero = Readings { displays: Some(0), ..readings(at(5, 9, 0)) };
        assert_eq!(Condition::Display { connected: true }.status(&one), Some(true));
        assert_eq!(Condition::Display { connected: true }.status(&zero), Some(false));
        assert_eq!(Condition::Display { connected: false }.status(&zero), Some(true));
        assert_eq!(Condition::Display { connected: false }.status(&readings(at(5, 9, 0))), None);
    }

    #[test]
    fn bluetooth_follows_one_device() {
        let names = vec!["Mouse".to_string(), "WH-1000XM5".to_string()];
        let r = Readings { bluetooth: Some(&names), ..readings(at(5, 9, 0)) };
        let device = |connected| Condition::Bluetooth { device: "WH-1000XM5".into(), connected };
        assert_eq!(device(true).status(&r), Some(true));
        assert_eq!(device(false).status(&r), Some(false));
        let other = Condition::Bluetooth { device: "Pen".into(), connected: false };
        assert_eq!(other.status(&r), Some(true));
    }

    #[test]
    fn battery_below_counts_whole_percents() {
        let below = Condition::BatteryBelow { percent: 20 };
        let pct = |p| Readings { battery_pct: Some(p), ..readings(at(5, 9, 0)) };
        assert_eq!(below.status(&pct(19.9)), Some(true));
        assert_eq!(below.status(&pct(20.4)), Some(false));
        assert_eq!(below.status(&readings(at(5, 9, 0))), None);
    }

    #[test]
    fn long_ac_counts_days_on_ac() {
        let long = Condition::LongAc { days: 3 };
        let since = Some(at(5, 10, 0));
        assert_eq!(long.status(&Readings { ac_since: since, ..readings(at(8, 9, 59)) }), Some(false));
        assert_eq!(long.status(&Readings { ac_since: since, ..readings(at(8, 10, 0)) }), Some(true));
        assert_eq!(long.status(&Readings { on_ac: false, ..readings(at(8, 10, 0)) }), Some(false));
    }

    #[test]
    fn a_moment_always_holds_here() {
        let am = Condition::Time { days: vec![0], time: "08:00".into(), until: None };
        assert!(am.is_moment());
        assert_eq!(am.status(&readings(at(10, 3, 0))), Some(true));
        let span = Condition::Time { days: vec![0], time: "08:00".into(), until: Some("09:00".into()) };
        assert!(!span.is_moment());
    }

    #[test]
    fn a_window_holds_from_its_start_to_its_end() {
        let mon = [0];
        assert!(!in_window(&mon, "09:00", "17:00", at(5, 8, 59)));
        assert!(in_window(&mon, "09:00", "17:00", at(5, 9, 0)));
        assert!(!in_window(&mon, "09:00", "17:00", at(5, 17, 0)));
        // Tuesday is not listed.
        assert!(!in_window(&mon, "09:00", "17:00", at(6, 10, 0)));
    }

    #[test]
    fn a_window_crosses_midnight_into_a_day_not_listed() {
        // 2026-10-09 is a Friday.
        let fri = [4];
        assert!(in_window(&fri, "22:00", "06:00", at(9, 23, 0)));
        assert!(in_window(&fri, "22:00", "06:00", at(10, 5, 59)));
        assert!(!in_window(&fri, "22:00", "06:00", at(10, 6, 0)));
        assert!(!in_window(&fri, "22:00", "06:00", at(11, 23, 0)));
    }

    #[test]
    fn a_between_condition_uses_its_window() {
        let span = Condition::Time { days: vec![0], time: "09:00".into(), until: Some("17:00".into()) };
        assert_eq!(span.status(&readings(at(5, 12, 0))), Some(true));
        assert_eq!(span.status(&readings(at(5, 18, 0))), Some(false));
    }

    #[test]
    fn all_hold_prefers_false_over_unknown() {
        let r = Readings { on_ac: true, ..readings(at(5, 9, 0)) };
        let yes = Condition::Power { plugged: true };
        let no = Condition::Power { plugged: false };
        let unknown = wifi("Office", true);
        assert_eq!(all_hold(&[yes.clone(), unknown.clone()], &r), None);
        assert_eq!(all_hold(&[no, unknown], &r), Some(false));
        assert_eq!(all_hold(&[yes], &r), Some(true));
        assert_eq!(all_hold(&[], &r), Some(true));
    }

    #[test]
    fn conditions_serialize_for_the_ui() {
        let at8 = Condition::Time { days: vec![0], time: "08:00".into(), until: None };
        assert_eq!(
            serde_json::to_value(&at8).unwrap(),
            serde_json::json!({ "kind": "time", "days": [0], "time": "08:00", "until": null })
        );
        let old: Condition = serde_json::from_str(r#"{"kind":"time","days":[0],"time":"08:00"}"#).unwrap();
        assert_eq!(old, at8);
        let below = serde_json::to_value(Condition::BatteryBelow { percent: 20 }).unwrap();
        assert_eq!(below, serde_json::json!({ "kind": "batteryBelow", "percent": 20 }));
    }
}
