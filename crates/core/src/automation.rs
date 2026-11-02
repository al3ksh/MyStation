//! Routines: conditions and what to change when they hold, stored in config.json, and a pure engine
//! the poller runs on every tick. Routines react only to events (plugging in, joining a Wi-Fi
//! network, a time passing), so a manual change holds until the next event. When several
//! routines fire at once, the one higher in the list wins each setting. A routine can restore what
//! it changed when its trigger ends (leaving the network, unplugging the display).

use std::collections::HashSet;
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::dell::ThermalMode;
use crate::power_mode::PowerMode;
use crate::profile::BatteryProfile;

mod condition;
mod engine;

pub use condition::{Condition, Readings, all_hold};
use condition::parse_time;
pub use engine::*;

/// `None` leaves the setting alone.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Actions {
    pub thermal: Option<ThermalMode>,
    pub battery: Option<BatteryProfile>,
    pub power_mode: Option<PowerMode>,
}

impl Actions {
    fn is_empty(&self) -> bool {
        self.thermal.is_none() && self.battery.is_none() && self.power_mode.is_none()
    }

    fn get(&self, slot: usize) -> Option<Change> {
        match slot {
            0 => self.thermal.map(Change::Thermal),
            1 => self.battery.map(Change::Battery),
            _ => self.power_mode.map(Change::PowerMode),
        }
    }

    fn put(&mut self, change: Change) {
        match change {
            Change::Thermal(m) => self.thermal = Some(m),
            Change::Battery(p) => self.battery = Some(p),
            Change::PowerMode(m) => self.power_mode = Some(m),
        }
    }

    fn clear(&mut self, slot: usize) {
        match slot {
            0 => self.thermal = None,
            1 => self.battery = None,
            _ => self.power_mode = None,
        }
    }

    /// Each setting from `self`, else from `other`.
    fn or(&self, other: &Actions) -> Actions {
        Actions {
            thermal: self.thermal.or(other.thermal),
            battery: self.battery.or(other.battery),
            power_mode: self.power_mode.or(other.power_mode),
        }
    }
}

const SLOTS: usize = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "RoutineFile")]
pub struct Routine {
    /// Stable across edits and reordering; the engine remembers per-routine state by it.
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// All must hold. None: a routine started by hand.
    pub conditions: Vec<Condition>,
    pub actions: Actions,
    /// A Windows notification when it runs.
    pub notify: bool,
    /// Put back what it changed when it ends.
    pub restore: bool,
}

/// A routine as stored: before conditions, a routine had one `trigger`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RoutineFile {
    id: String,
    name: String,
    enabled: bool,
    #[serde(default)]
    conditions: Vec<Condition>,
    #[serde(default)]
    trigger: Option<Condition>,
    actions: Actions,
    notify: bool,
    #[serde(default)]
    restore: bool,
}

impl From<RoutineFile> for Routine {
    fn from(f: RoutineFile) -> Self {
        let conditions = if f.conditions.is_empty() { f.trigger.into_iter().collect() } else { f.conditions };
        Routine { id: f.id, name: f.name, enabled: f.enabled, conditions, actions: f.actions, notify: f.notify, restore: f.restore }
    }
}

impl Routine {
    /// No conditions: it starts and stops by hand.
    pub fn is_manual(&self) -> bool {
        self.conditions.is_empty()
    }

    /// "At" a time: it runs at that moment and never over time.
    pub fn is_instant(&self) -> bool {
        self.conditions.iter().any(Condition::is_moment)
    }
}

pub const LONG_AC_DAYS: RangeInclusive<u32> = 1..=60;
pub const BATTERY_BELOW: RangeInclusive<u8> = 5..=95;
const NAME_MAX: usize = 60;

fn validate_condition(name: &str, c: &Condition) -> Result<(), String> {
    match c {
        Condition::Time { days, time, until } => {
            if days.is_empty() || days.iter().any(|&d| d > 6) {
                return Err(format!("Pick at least one day for {name}."));
            }
            let Some(start) = parse_time(time) else { return Err(format!("Invalid time: {time}.")) };
            if let Some(until) = until {
                let Some(end) = parse_time(until) else { return Err(format!("Invalid time: {until}.")) };
                if end == start {
                    return Err(format!("{name} has to end at another time than it starts."));
                }
            }
        }
        Condition::Wifi { ssid, .. } if ssid.trim().is_empty() || ssid.len() > 32 => {
            return Err(format!("Enter the Wi-Fi network for {name}."));
        }
        Condition::Bluetooth { device, .. } if device.trim().is_empty() => {
            return Err(format!("Pick the Bluetooth device for {name}."));
        }
        Condition::BatteryBelow { percent } if !BATTERY_BELOW.contains(percent) => {
            return Err(format!(
                "The battery level must be between {}% and {}%.",
                BATTERY_BELOW.start(),
                BATTERY_BELOW.end()
            ));
        }
        Condition::LongAc { days } if !LONG_AC_DAYS.contains(days) => {
            return Err(format!("Days on AC must be between {} and {}.", LONG_AC_DAYS.start(), LONG_AC_DAYS.end()));
        }
        _ => {}
    }
    Ok(())
}

/// Rejects routines the UI should never send; the message reaches the user as is.
pub fn validate(routines: &[Routine]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for r in routines {
        if r.id.is_empty() || !ids.insert(r.id.as_str()) {
            return Err("Each routine needs its own id.".into());
        }
        let name = r.name.trim();
        if name.is_empty() {
            return Err("Give each routine a name.".into());
        }
        if name.chars().count() > NAME_MAX {
            return Err(format!("Keep routine names to {NAME_MAX} characters."));
        }
        for (i, c) in r.conditions.iter().enumerate() {
            let kind = std::mem::discriminant(c);
            if r.conditions[..i].iter().any(|d| std::mem::discriminant(d) == kind) {
                return Err(format!("{name} has two {} conditions.", c.label()));
            }
            validate_condition(name, c)?;
        }
        let on_battery = r.conditions.contains(&Condition::Power { plugged: false });
        if on_battery && r.conditions.iter().any(|c| matches!(c, Condition::LongAc { .. })) {
            return Err(format!("{name} can never run: it needs days on AC while on battery."));
        }
        if r.actions.is_empty() && !r.notify {
            return Err(format!("{name} doesn't change anything."));
        }
    }
    Ok(())
}

// The fixed rules before routines, kept to read and migrate older config files.

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThermalRule {
    pub enabled: bool,
    pub on_ac: Option<ThermalMode>,
    pub on_battery: Option<ThermalMode>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleEntry {
    pub days: Vec<u8>,
    pub time: String,
    pub profile: BatteryProfile,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduleRule {
    pub enabled: bool,
    pub entries: Vec<ScheduleEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LongAcRule {
    pub enabled: bool,
    pub days: u32,
    pub profile: BatteryProfile,
}

impl Default for LongAcRule {
    fn default() -> Self {
        Self { enabled: false, days: 3, profile: BatteryProfile::Storage }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Automation {
    pub thermal: ThermalRule,
    pub schedule: ScheduleRule,
    pub long_ac: LongAcRule,
    pub notify: bool,
}

impl Default for Automation {
    fn default() -> Self {
        Self {
            thermal: ThermalRule::default(),
            schedule: ScheduleRule::default(),
            long_ac: LongAcRule::default(),
            notify: true,
        }
    }
}

fn profile_label(profile: BatteryProfile) -> &'static str {
    match profile {
        BatteryProfile::Home => "Desk",
        BatteryProfile::Campus => "Remote",
        BatteryProfile::Storage => "Storage",
    }
}

/// The old rules as routines, in the order that keeps their priorities: long AC beat the schedule.
pub fn migrate(old: &Automation) -> Vec<Routine> {
    let routine = |id: String, name: String, enabled: bool, condition: Condition, actions: Actions| Routine {
        id,
        name,
        enabled,
        conditions: vec![condition],
        actions,
        notify: old.notify,
        restore: false,
    };
    let mut out = Vec::new();
    for (plugged, mode, id, name) in
        [(true, old.thermal.on_ac, "plugged-in", "Plugged in"), (false, old.thermal.on_battery, "on-battery", "On battery")]
    {
        if let Some(mode) = mode {
            let actions = Actions { thermal: Some(mode), ..Actions::default() };
            out.push(routine(id.into(), name.into(), old.thermal.enabled, Condition::Power { plugged }, actions));
        }
    }
    if old.long_ac.enabled {
        let days = old.long_ac.days;
        let actions = Actions { battery: Some(old.long_ac.profile), ..Actions::default() };
        let name = if days == 1 { "Plugged in for a day".into() } else { format!("Plugged in for {days} days") };
        out.push(routine("long-ac".into(), name, true, Condition::LongAc { days }, actions));
    }
    for (i, e) in old.schedule.entries.iter().enumerate() {
        let name = format!("{} at {}", profile_label(e.profile), e.time);
        let actions = Actions { battery: Some(e.profile), ..Actions::default() };
        let time = Condition::Time { days: e.days.clone(), time: e.time.clone(), until: None };
        out.push(routine(format!("schedule-{i}"), name, old.schedule.enabled, time, actions));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dell::ThermalMode;

    fn routine(id: &str, condition: Condition, actions: Actions) -> Routine {
        Routine { id: id.into(), name: id.into(), enabled: true, conditions: vec![condition], actions, notify: true, restore: false }
    }

    fn thermal(mode: ThermalMode) -> Actions {
        Actions { thermal: Some(mode), ..Actions::default() }
    }

    fn battery(profile: BatteryProfile) -> Actions {
        Actions { battery: Some(profile), ..Actions::default() }
    }

    fn power(plugged: bool) -> Condition {
        Condition::Power { plugged }
    }

    fn time(days: &[u8], t: &str) -> Condition {
        Condition::Time { days: days.to_vec(), time: t.into(), until: None }
    }

    fn wifi(ssid: &str, connected: bool) -> Condition {
        Condition::Wifi { ssid: ssid.into(), connected }
    }

    #[test]
    fn validate_rejects_bad_routines() {
        let ok = routine("a", time(&[0], "08:00"), battery(BatteryProfile::Home));
        assert!(validate(std::slice::from_ref(&ok)).is_ok());
        let bad = |f: &dyn Fn(&mut Routine)| {
            let mut r = ok.clone();
            f(&mut r);
            validate(&[r]).is_err()
        };
        assert!(bad(&|r| r.conditions = vec![time(&[], "08:00")]));
        assert!(bad(&|r| r.conditions = vec![time(&[7], "08:00")]));
        assert!(bad(&|r| r.conditions = vec![time(&[0], "8am")]));
        assert!(bad(&|r| r.name = "  ".into()));
        assert!(bad(&|r| r.conditions = vec![wifi(" ", true)]));
        assert!(bad(&|r| r.conditions = vec![Condition::Bluetooth { device: String::new(), connected: true }]));
        assert!(bad(&|r| r.conditions = vec![Condition::BatteryBelow { percent: 2 }]));
        assert!(bad(&|r| r.conditions = vec![Condition::LongAc { days: 0 }]));
        assert!(bad(&|r| {
            r.actions = Actions::default();
            r.notify = false;
        }));
        let between = |until: &str| Condition::Time { days: vec![0], time: "08:00".into(), until: Some(until.into()) };
        assert!(!bad(&|r| r.conditions = vec![between("09:00")]));
        assert!(bad(&|r| r.conditions = vec![between("08:00")]));
        assert!(bad(&|r| r.conditions = vec![between("25:00")]));
        assert!(bad(&|r| r.conditions = vec![wifi("Office", true), wifi("Home", true)]));
        assert!(bad(&|r| r.conditions = vec![power(false), Condition::LongAc { days: 3 }]));
        assert!(!bad(&|r| r.conditions = vec![power(true), wifi("Office", true)]));
        // Started by hand: no conditions.
        assert!(!bad(&|r| r.conditions = vec![]));
        assert!(validate(&[ok.clone(), ok]).is_err());
    }

    #[test]
    fn routines_serialize_for_the_ui() {
        let r = routine("w", wifi("Office", true), Actions { power_mode: Some(PowerMode::Balanced), ..Actions::default() });
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["conditions"][0], serde_json::json!({ "kind": "wifi", "ssid": "Office", "connected": true }));
        assert!(json.get("trigger").is_none());
        assert_eq!(json["actions"]["powerMode"], "Balanced");
        assert_eq!(json["actions"]["thermal"], serde_json::Value::Null);
        assert_eq!(json["restore"], false);
        let mut old = json.clone();
        old.as_object_mut().unwrap().remove("restore");
        assert_eq!(serde_json::from_value::<Routine>(old).unwrap(), r);
    }

    #[test]
    fn legacy_trigger_reads_as_one_condition() {
        let old = r#"{"id":"a","name":"A","enabled":true,"trigger":{"kind":"time","days":[0],"time":"08:00"},"actions":{},"notify":false}"#;
        let r: Routine = serde_json::from_str(old).unwrap();
        assert_eq!(r.conditions, vec![time(&[0], "08:00")]);
        assert!(!r.restore);
        assert!(r.is_instant());
        assert!(!r.is_manual());
    }

    #[test]
    fn migration_turns_the_old_rules_into_routines() {
        let old = Automation {
            thermal: ThermalRule { enabled: true, on_ac: Some(ThermalMode::UltraPerformance), on_battery: None },
            schedule: ScheduleRule {
                enabled: false,
                entries: vec![ScheduleEntry { days: vec![0, 1], time: "08:00".into(), profile: BatteryProfile::Campus }],
            },
            long_ac: LongAcRule { enabled: true, days: 5, profile: BatteryProfile::Storage },
            notify: false,
        };
        let routines = migrate(&old);
        assert_eq!(
            routines,
            vec![
                Routine {
                    id: "plugged-in".into(),
                    name: "Plugged in".into(),
                    enabled: true,
                    conditions: vec![power(true)],
                    actions: thermal(ThermalMode::UltraPerformance),
                    notify: false,
                    restore: false,
                },
                Routine {
                    id: "long-ac".into(),
                    name: "Plugged in for 5 days".into(),
                    enabled: true,
                    conditions: vec![Condition::LongAc { days: 5 }],
                    actions: battery(BatteryProfile::Storage),
                    notify: false,
                    restore: false,
                },
                Routine {
                    id: "schedule-0".into(),
                    name: "Remote at 08:00".into(),
                    enabled: false,
                    conditions: vec![time(&[0, 1], "08:00")],
                    actions: battery(BatteryProfile::Campus),
                    notify: false,
                    restore: false,
                },
            ]
        );
        assert!(validate(&routines).is_ok());
        assert_eq!(migrate(&Automation::default()), vec![]);
    }

}
