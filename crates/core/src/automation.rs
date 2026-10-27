//! Routines: a trigger and what to change when it fires, stored in config.json, and a pure engine
//! the poller runs on every tick. Routines react only to events (plugging in, joining a Wi-Fi
//! network, a time passing), so a manual change holds until the next event. When several
//! routines fire at once, the one higher in the list wins each setting.

use std::collections::HashSet;
use std::ops::RangeInclusive;

use chrono::{Datelike, Duration, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::dell::ThermalMode;
use crate::power_mode::PowerMode;
use crate::profile::BatteryProfile;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Trigger {
    Power { plugged: bool },
    /// A moment, not a range. Days: 0 = Monday … 6 = Sunday; time: local `HH:MM`.
    Time { days: Vec<u8>, time: String },
    Wifi { ssid: String, connected: bool },
    /// Any display besides the built-in one, a dock's included.
    Display { connected: bool },
    Bluetooth { device: String, connected: bool },
    BatteryBelow { percent: u8 },
    /// On AC without a break for this many days; fires once per AC period.
    LongAc { days: u32 },
}

fn parse_time(time: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(time, "%H:%M").ok()
}

impl Trigger {
    /// What happened, for the notification.
    pub fn describe(&self) -> String {
        match self {
            Trigger::Power { plugged: true } => "Plugged in".into(),
            Trigger::Power { plugged: false } => "On battery".into(),
            Trigger::Time { time, .. } => format!("It's {time}"),
            Trigger::Wifi { ssid, connected: true } => format!("Connected to {ssid}"),
            Trigger::Wifi { ssid, connected: false } => format!("Disconnected from {ssid}"),
            Trigger::Display { connected: true } => "External display connected".into(),
            Trigger::Display { connected: false } => "External display disconnected".into(),
            Trigger::Bluetooth { device, connected: true } => format!("{device} connected"),
            Trigger::Bluetooth { device, connected: false } => format!("{device} disconnected"),
            Trigger::BatteryBelow { percent } => format!("Battery below {percent}%"),
            Trigger::LongAc { days: 1 } => "Plugged in for a day".into(),
            Trigger::LongAc { days } => format!("Plugged in for {days} days"),
        }
    }
}

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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    /// Stable across edits and reordering; the engine remembers per-routine state by it.
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub actions: Actions,
    /// A Windows notification when it runs.
    pub notify: bool,
}

pub const LONG_AC_DAYS: RangeInclusive<u32> = 1..=60;
pub const BATTERY_BELOW: RangeInclusive<u8> = 5..=95;
const NAME_MAX: usize = 60;

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
        match &r.trigger {
            Trigger::Time { days, time } => {
                if days.is_empty() || days.iter().any(|&d| d > 6) {
                    return Err(format!("Pick at least one day for {name}."));
                }
                if parse_time(time).is_none() {
                    return Err(format!("Invalid time: {time}."));
                }
            }
            Trigger::Wifi { ssid, .. } if ssid.trim().is_empty() || ssid.len() > 32 => {
                return Err(format!("Enter the Wi-Fi network for {name}."));
            }
            Trigger::Bluetooth { device, .. } if device.trim().is_empty() => {
                return Err(format!("Pick the Bluetooth device for {name}."));
            }
            Trigger::BatteryBelow { percent } if !BATTERY_BELOW.contains(percent) => {
                return Err(format!(
                    "The battery level must be between {}% and {}%.",
                    BATTERY_BELOW.start(),
                    BATTERY_BELOW.end()
                ));
            }
            Trigger::LongAc { days } if !LONG_AC_DAYS.contains(days) => {
                return Err(format!(
                    "Days on AC must be between {} and {}.",
                    LONG_AC_DAYS.start(),
                    LONG_AC_DAYS.end()
                ));
            }
            _ => {}
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
    let routine = |id: String, name: String, enabled: bool, trigger: Trigger, actions: Actions| Routine {
        id,
        name,
        enabled,
        trigger,
        actions,
        notify: old.notify,
    };
    let mut out = Vec::new();
    for (plugged, mode, id, name) in
        [(true, old.thermal.on_ac, "plugged-in", "Plugged in"), (false, old.thermal.on_battery, "on-battery", "On battery")]
    {
        if let Some(mode) = mode {
            let actions = Actions { thermal: Some(mode), ..Actions::default() };
            out.push(routine(id.into(), name.into(), old.thermal.enabled, Trigger::Power { plugged }, actions));
        }
    }
    if old.long_ac.enabled {
        let days = old.long_ac.days;
        let actions = Actions { battery: Some(old.long_ac.profile), ..Actions::default() };
        let trigger = Trigger::LongAc { days };
        out.push(routine("long-ac".into(), trigger.describe(), true, trigger, actions));
    }
    for (i, e) in old.schedule.entries.iter().enumerate() {
        let name = format!("{} at {}", profile_label(e.profile), e.time);
        let actions = Actions { battery: Some(e.profile), ..Actions::default() };
        let trigger = Trigger::Time { days: e.days.clone(), time: e.time.clone() };
        out.push(routine(format!("schedule-{i}"), name, old.schedule.enabled, trigger, actions));
    }
    out
}

/// What the engine remembers between ticks and across restarts (`automation-state.json`).
/// Every input starts unseen: its first reading is recorded, not treated as a change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutomationState {
    pub on_ac: Option<bool>,
    /// Start of the current continuous AC period.
    pub ac_since: Option<NaiveDateTime>,
    /// Long-AC routines that already ran in this AC period.
    pub long_ac_done: Vec<String>,
    /// The last time moment handled; later moments are due.
    pub last_schedule: Option<NaiveDateTime>,
    pub wifi_seen: bool,
    pub ssid: Option<String>,
    pub displays: Option<u32>,
    pub bluetooth: Option<Vec<String>>,
    pub battery_pct: Option<u8>,
}

impl AutomationState {
    /// After the routines change, only moments from now on count: editing never fires a past one.
    pub fn rules_changed(&mut self, now: NaiveDateTime) {
        self.last_schedule = Some(now);
    }
}

/// Missing or unreadable state starts fresh: the next tick only records the inputs.
pub fn load_state(path: &std::path::Path) -> AutomationState {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_state(path: &std::path::Path, state: &AutomationState) -> std::io::Result<()> {
    crate::config::write_atomic(path, serde_json::to_string_pretty(state)?.as_bytes())
}

/// One reading of everything routines watch. `None` for an input not read this time: nothing
/// changes for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Inputs {
    pub now: NaiveDateTime,
    pub on_ac: bool,
    pub battery_pct: Option<f32>,
    /// `Some(None)`: not connected to Wi-Fi.
    pub wifi: Option<Option<String>>,
    /// External displays.
    pub displays: Option<u32>,
    /// Names of the connected Bluetooth devices.
    pub bluetooth: Option<Vec<String>>,
}

impl Inputs {
    pub fn at(now: NaiveDateTime, on_ac: bool) -> Self {
        Self { now, on_ac, battery_pct: None, wifi: None, displays: None, bluetooth: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Thermal(ThermalMode),
    Battery(BatteryProfile),
    PowerMode(PowerMode),
}

/// A routine that fired and the changes it won.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub id: String,
    pub changes: Vec<Change>,
}

/// The latest moment a time trigger passed at or before `now`, looking back a week.
fn last_moment(days: &[u8], time: &str, now: NaiveDateTime) -> Option<NaiveDateTime> {
    let time = parse_time(time)?;
    (0..=7)
        .map(|back| (now.date() - Duration::days(back)).and_time(time))
        .find(|m| *m <= now && days.contains(&(m.weekday().num_days_from_monday() as u8)))
}

fn time_moment(r: &Routine, now: NaiveDateTime) -> Option<NaiveDateTime> {
    match &r.trigger {
        Trigger::Time { days, time } if r.enabled => last_moment(days, time, now),
        _ => None,
    }
}

/// One engine step. Mutates `state`; the caller persists it when it changed.
pub fn evaluate(state: &mut AutomationState, input: &Inputs, routines: &[Routine]) -> Vec<Run> {
    let now = input.now;
    let mut fired = vec![false; routines.len()];
    let fire = |fired: &mut Vec<bool>, pred: &dyn Fn(&Trigger) -> bool| {
        for (i, r) in routines.iter().enumerate() {
            if r.enabled && pred(&r.trigger) {
                fired[i] = true;
            }
        }
    };
    let mut restore = false;

    if state.on_ac != Some(input.on_ac) {
        // The first reading ever has nothing to compare with: not an event.
        if state.on_ac.is_some() {
            fire(&mut fired, &|t| *t == Trigger::Power { plugged: input.on_ac });
        }
        state.on_ac = Some(input.on_ac);
        if input.on_ac {
            state.ac_since = Some(now);
        } else {
            // Leaving AC ends the long-AC stretch: back to whatever the time routines say now.
            restore = !state.long_ac_done.is_empty();
            state.ac_since = None;
        }
        state.long_ac_done.clear();
    }

    if input.on_ac
        && let Some(since) = state.ac_since
    {
        for (i, r) in routines.iter().enumerate() {
            if let Trigger::LongAc { days } = r.trigger
                && r.enabled
                && !state.long_ac_done.contains(&r.id)
                && now - since >= Duration::days(days.into())
            {
                state.long_ac_done.push(r.id.clone());
                fired[i] = true;
            }
        }
    }

    let latest = routines.iter().filter_map(|r| time_moment(r, now)).max();
    match state.last_schedule {
        None => state.last_schedule = Some(now),
        Some(last) => {
            if let Some(latest) = latest
                && latest > last
            {
                state.last_schedule = Some(latest);
                // After a gap only the latest missed moment runs.
                for (i, r) in routines.iter().enumerate() {
                    if time_moment(r, now) == Some(latest) {
                        fired[i] = true;
                    }
                }
            }
        }
    }

    if let Some(ssid) = &input.wifi {
        if state.wifi_seen && state.ssid != *ssid {
            let before = state.ssid.clone();
            fire(&mut fired, &|t| match t {
                Trigger::Wifi { ssid: s, connected: true } => ssid.as_ref() == Some(s) && before.as_ref() != Some(s),
                Trigger::Wifi { ssid: s, connected: false } => before.as_ref() == Some(s) && ssid.as_ref() != Some(s),
                _ => false,
            });
        }
        state.wifi_seen = true;
        state.ssid = ssid.clone();
    }

    if let Some(n) = input.displays {
        if let Some(before) = state.displays
            && (before == 0) != (n == 0)
        {
            fire(&mut fired, &|t| *t == Trigger::Display { connected: n > 0 });
        }
        state.displays = Some(n);
    }

    if let Some(names) = &input.bluetooth {
        if let Some(before) = &state.bluetooth {
            fire(&mut fired, &|t| match t {
                Trigger::Bluetooth { device, connected: true } => names.contains(device) && !before.contains(device),
                Trigger::Bluetooth { device, connected: false } => before.contains(device) && !names.contains(device),
                _ => false,
            });
        }
        state.bluetooth = Some(names.clone());
    }

    if let Some(pct) = input.battery_pct {
        // Whole percents: the file changes once per percent, not on every reading.
        let pct = pct.clamp(0.0, 100.0) as u8;
        if let Some(before) = state.battery_pct {
            fire(&mut fired, &|t| matches!(t, Trigger::BatteryBelow { percent } if before >= *percent && pct < *percent));
        }
        state.battery_pct = Some(pct);
    }

    // While a long-AC routine holds the battery profile, time routines leave it alone.
    let hold = input.on_ac && !state.long_ac_done.is_empty();
    let (mut thermal, mut battery, mut power_mode) = (false, false, false);
    let mut runs = Vec::new();
    for (r, _) in routines.iter().zip(&fired).filter(|(_, f)| **f) {
        let mut changes = Vec::new();
        if let Some(mode) = r.actions.thermal
            && !std::mem::replace(&mut thermal, true)
        {
            changes.push(Change::Thermal(mode));
        }
        let held = hold && matches!(r.trigger, Trigger::Time { .. });
        if let Some(profile) = r.actions.battery
            && !held
            && !std::mem::replace(&mut battery, true)
        {
            changes.push(Change::Battery(profile));
        }
        if let Some(mode) = r.actions.power_mode
            && !std::mem::replace(&mut power_mode, true)
        {
            changes.push(Change::PowerMode(mode));
        }
        runs.push(Run { id: r.id.clone(), changes });
    }

    if restore && !battery {
        let latest = routines
            .iter()
            .filter(|r| r.actions.battery.is_some())
            .filter_map(|r| time_moment(r, now).map(|m| (m, r)))
            .max_by_key(|(m, _)| *m);
        if let Some((_, r)) = latest
            && let Some(profile) = r.actions.battery
        {
            runs.push(Run { id: r.id.clone(), changes: vec![Change::Battery(profile)] });
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    // 2026-10-05 is a Monday.
    fn at(day: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn routine(id: &str, trigger: Trigger, actions: Actions) -> Routine {
        Routine { id: id.into(), name: id.into(), enabled: true, trigger, actions, notify: true }
    }

    fn thermal(mode: ThermalMode) -> Actions {
        Actions { thermal: Some(mode), ..Actions::default() }
    }

    fn battery(profile: BatteryProfile) -> Actions {
        Actions { battery: Some(profile), ..Actions::default() }
    }

    fn power(plugged: bool) -> Trigger {
        Trigger::Power { plugged }
    }

    fn time(days: &[u8], t: &str) -> Trigger {
        Trigger::Time { days: days.to_vec(), time: t.into() }
    }

    const EVERY_DAY: &[u8] = &[0, 1, 2, 3, 4, 5, 6];

    fn plug_rules() -> Vec<Routine> {
        vec![
            routine("ac", power(true), thermal(ThermalMode::UltraPerformance)),
            routine("bat", power(false), thermal(ThermalMode::Quiet)),
        ]
    }

    fn seen(on_ac: bool, now: NaiveDateTime) -> AutomationState {
        AutomationState { on_ac: Some(on_ac), ac_since: on_ac.then_some(now), last_schedule: Some(now), ..Default::default() }
    }

    fn run(id: &str, changes: Vec<Change>) -> Run {
        Run { id: id.into(), changes }
    }

    fn step(s: &mut AutomationState, on_ac: bool, now: NaiveDateTime, rules: &[Routine]) -> Vec<Run> {
        evaluate(s, &Inputs::at(now, on_ac), rules)
    }

    #[test]
    fn first_tick_only_records_the_power_source() {
        let mut s = AutomationState::default();
        assert_eq!(step(&mut s, true, at(5, 10, 0), &plug_rules()), vec![]);
        assert_eq!(s.on_ac, Some(true));
        assert_eq!(s.ac_since, Some(at(5, 10, 0)));
        assert_eq!(s.last_schedule, Some(at(5, 10, 0)));
    }

    #[test]
    fn unplugging_runs_the_battery_routine() {
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(
            step(&mut s, false, at(5, 10, 0), &plug_rules()),
            vec![run("bat", vec![Change::Thermal(ThermalMode::Quiet)])]
        );
        assert_eq!(s.ac_since, None);
    }

    #[test]
    fn plugging_in_runs_the_ac_routine_and_starts_the_ac_clock() {
        let mut s = seen(false, at(5, 9, 0));
        assert_eq!(
            step(&mut s, true, at(5, 10, 0), &plug_rules()),
            vec![run("ac", vec![Change::Thermal(ThermalMode::UltraPerformance)])]
        );
        assert_eq!(s.ac_since, Some(at(5, 10, 0)));
    }

    #[test]
    fn disabled_routine_does_nothing() {
        let mut rules = plug_rules();
        rules[1].enabled = false;
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(step(&mut s, false, at(5, 10, 0), &rules), vec![]);
    }

    #[test]
    fn steady_power_source_is_not_an_event() {
        let mut s = seen(false, at(5, 9, 0));
        assert_eq!(step(&mut s, false, at(5, 10, 0), &plug_rules()), vec![]);
    }

    #[test]
    fn higher_routine_wins_each_setting_and_lower_ones_keep_the_rest() {
        let both = Actions { thermal: Some(ThermalMode::Cool), power_mode: Some(PowerMode::BestEfficiency), ..Actions::default() };
        let rules = vec![
            routine("first", power(false), thermal(ThermalMode::Quiet)),
            routine("second", power(false), both),
        ];
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(
            step(&mut s, false, at(5, 10, 0), &rules),
            vec![
                run("first", vec![Change::Thermal(ThermalMode::Quiet)]),
                run("second", vec![Change::PowerMode(PowerMode::BestEfficiency)]),
            ]
        );
    }

    #[test]
    fn a_passed_time_runs_its_routine_once() {
        let rules = vec![routine("am", time(&[0, 1, 2, 3, 4], "08:00"), battery(BatteryProfile::Campus))];
        let mut s = seen(false, at(5, 7, 59));
        assert_eq!(step(&mut s, false, at(5, 8, 0), &rules), vec![run("am", vec![Change::Battery(BatteryProfile::Campus)])]);
        assert_eq!(step(&mut s, false, at(5, 8, 1), &rules), vec![]);
    }

    #[test]
    fn time_skips_days_not_listed() {
        // 2026-10-10 is a Saturday.
        let rules = vec![routine("am", time(&[0, 1, 2, 3, 4], "08:00"), battery(BatteryProfile::Campus))];
        let mut s = seen(false, at(10, 7, 59));
        assert_eq!(step(&mut s, false, at(10, 8, 0), &rules), vec![]);
    }

    #[test]
    fn time_crosses_midnight() {
        let rules = vec![routine("late", time(&[0], "23:59"), battery(BatteryProfile::Home))];
        let mut s = seen(false, at(5, 23, 58));
        assert_eq!(step(&mut s, false, at(6, 0, 1), &rules), vec![run("late", vec![Change::Battery(BatteryProfile::Home)])]);
    }

    #[test]
    fn after_a_restart_only_the_latest_missed_time_runs() {
        let rules = vec![
            routine("am", time(EVERY_DAY, "08:00"), battery(BatteryProfile::Campus)),
            routine("pm", time(EVERY_DAY, "18:00"), battery(BatteryProfile::Home)),
        ];
        // Saved Monday evening, back on Wednesday at noon.
        let mut s = AutomationState { on_ac: Some(false), last_schedule: Some(at(5, 20, 0)), ..Default::default() };
        assert_eq!(step(&mut s, false, at(7, 12, 0), &rules), vec![run("am", vec![Change::Battery(BatteryProfile::Campus)])]);
        assert_eq!(step(&mut s, false, at(7, 12, 1), &rules), vec![]);
    }

    #[test]
    fn editing_the_routines_never_fires_a_past_time() {
        let rules = vec![routine("am", time(&[0], "08:00"), battery(BatteryProfile::Campus))];
        let mut s = AutomationState { on_ac: Some(false), last_schedule: Some(at(4, 20, 0)), ..Default::default() };
        s.rules_changed(at(5, 10, 0));
        assert_eq!(step(&mut s, false, at(5, 10, 1), &rules), vec![]);
    }

    fn long_ac(days: u32) -> Routine {
        routine("long", Trigger::LongAc { days }, battery(BatteryProfile::Storage))
    }

    #[test]
    fn long_ac_runs_once_per_ac_period() {
        let rules = vec![long_ac(3)];
        let mut s = seen(true, at(5, 10, 0));
        assert_eq!(step(&mut s, true, at(8, 9, 59), &rules), vec![]);
        assert_eq!(step(&mut s, true, at(8, 10, 0), &rules), vec![run("long", vec![Change::Battery(BatteryProfile::Storage)])]);
        assert_eq!(step(&mut s, true, at(9, 10, 0), &rules), vec![]);
    }

    #[test]
    fn long_ac_holds_the_profile_against_time_routines_while_on_ac() {
        let rules = vec![long_ac(3), routine("am", time(EVERY_DAY, "08:00"), battery(BatteryProfile::Home))];
        let mut s = seen(true, at(5, 7, 0));
        step(&mut s, true, at(8, 7, 0), &rules);
        assert_eq!(s.long_ac_done, vec!["long".to_string()]);
        assert_eq!(step(&mut s, true, at(8, 8, 0), &rules), vec![run("am", vec![])]);
    }

    #[test]
    fn unplugging_after_long_ac_restores_the_time_routine_profile() {
        let rules = vec![long_ac(3), routine("am", time(EVERY_DAY, "08:00"), battery(BatteryProfile::Home))];
        let mut s = seen(true, at(5, 7, 0));
        step(&mut s, true, at(8, 7, 0), &rules);
        step(&mut s, true, at(8, 11, 0), &rules);
        assert_eq!(step(&mut s, false, at(8, 12, 0), &rules), vec![run("am", vec![Change::Battery(BatteryProfile::Home)])]);
        assert!(s.long_ac_done.is_empty());
    }

    #[test]
    fn a_battery_routine_on_unplug_beats_the_restore() {
        let rules = vec![
            long_ac(3),
            routine("am", time(EVERY_DAY, "08:00"), battery(BatteryProfile::Home)),
            routine("bat", power(false), battery(BatteryProfile::Campus)),
        ];
        let mut s = seen(true, at(5, 7, 0));
        step(&mut s, true, at(8, 7, 0), &rules);
        step(&mut s, true, at(8, 11, 0), &rules);
        assert_eq!(step(&mut s, false, at(8, 12, 0), &rules), vec![run("bat", vec![Change::Battery(BatteryProfile::Campus)])]);
    }

    #[test]
    fn a_new_ac_period_restarts_the_long_ac_clock() {
        let rules = vec![long_ac(3)];
        let mut s = seen(true, at(5, 10, 0));
        step(&mut s, false, at(7, 10, 0), &rules);
        step(&mut s, true, at(7, 11, 0), &rules);
        assert_eq!(step(&mut s, true, at(8, 10, 0), &rules), vec![]);
    }

    fn wifi(ssid: &str, connected: bool) -> Trigger {
        Trigger::Wifi { ssid: ssid.into(), connected }
    }

    fn on_wifi(now: NaiveDateTime, ssid: Option<&str>) -> Inputs {
        Inputs { wifi: Some(ssid.map(String::from)), ..Inputs::at(now, true) }
    }

    #[test]
    fn wifi_routines_run_when_joining_and_leaving_a_network() {
        let rules = vec![
            routine("join", wifi("Office", true), thermal(ThermalMode::Optimized)),
            routine("leave", wifi("Office", false), thermal(ThermalMode::Quiet)),
        ];
        let mut s = seen(true, at(5, 9, 0));
        // The first reading is only recorded.
        assert_eq!(evaluate(&mut s, &on_wifi(at(5, 9, 0), Some("Home")), &rules), vec![]);
        assert_eq!(
            evaluate(&mut s, &on_wifi(at(5, 9, 1), Some("Office")), &rules),
            vec![run("join", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
        assert_eq!(evaluate(&mut s, &on_wifi(at(5, 9, 2), Some("Office")), &rules), vec![]);
        assert_eq!(
            evaluate(&mut s, &on_wifi(at(5, 9, 3), None), &rules),
            vec![run("leave", vec![Change::Thermal(ThermalMode::Quiet)])]
        );
    }

    #[test]
    fn an_unread_input_changes_nothing() {
        let rules = vec![routine("leave", wifi("Office", false), thermal(ThermalMode::Quiet))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &on_wifi(at(5, 9, 0), Some("Office")), &rules);
        assert_eq!(step(&mut s, true, at(5, 9, 1), &rules), vec![]);
        assert_eq!(s.ssid.as_deref(), Some("Office"));
    }

    #[test]
    fn display_routines_run_on_the_first_and_last_external_display() {
        let rules = vec![
            routine("dock", Trigger::Display { connected: true }, thermal(ThermalMode::UltraPerformance)),
            routine("undock", Trigger::Display { connected: false }, thermal(ThermalMode::Optimized)),
        ];
        let displays = |now, n| Inputs { displays: Some(n), ..Inputs::at(now, true) };
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(evaluate(&mut s, &displays(at(5, 9, 0), 0), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &displays(at(5, 9, 1), 1), &rules), vec![run("dock", vec![Change::Thermal(ThermalMode::UltraPerformance)])]);
        assert_eq!(evaluate(&mut s, &displays(at(5, 9, 2), 2), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &displays(at(5, 9, 3), 0), &rules), vec![run("undock", vec![Change::Thermal(ThermalMode::Optimized)])]);
    }

    #[test]
    fn bluetooth_routines_follow_one_device() {
        let device = |connected| Trigger::Bluetooth { device: "WH-1000XM5".into(), connected };
        let rules = vec![
            routine("on", device(true), thermal(ThermalMode::Quiet)),
            routine("off", device(false), thermal(ThermalMode::Optimized)),
        ];
        let bt = |now, names: &[&str]| Inputs {
            bluetooth: Some(names.iter().map(|n| n.to_string()).collect()),
            ..Inputs::at(now, true)
        };
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(evaluate(&mut s, &bt(at(5, 9, 0), &["Mouse"]), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &bt(at(5, 9, 1), &["Mouse", "WH-1000XM5"]), &rules), vec![run("on", vec![Change::Thermal(ThermalMode::Quiet)])]);
        assert_eq!(evaluate(&mut s, &bt(at(5, 9, 2), &["WH-1000XM5"]), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &bt(at(5, 9, 3), &[]), &rules), vec![run("off", vec![Change::Thermal(ThermalMode::Optimized)])]);
    }

    #[test]
    fn battery_below_runs_when_the_level_drops_past_it() {
        let rules = vec![routine("low", Trigger::BatteryBelow { percent: 20 }, thermal(ThermalMode::Quiet))];
        let pct = |now, p| Inputs { battery_pct: Some(p), ..Inputs::at(now, false) };
        let mut s = seen(false, at(5, 9, 0));
        assert_eq!(evaluate(&mut s, &pct(at(5, 9, 0), 21.0), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &pct(at(5, 9, 1), 20.4), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &pct(at(5, 9, 2), 19.9), &rules), vec![run("low", vec![Change::Thermal(ThermalMode::Quiet)])]);
        assert_eq!(evaluate(&mut s, &pct(at(5, 9, 3), 18.0), &rules), vec![]);
    }

    #[test]
    fn a_notify_only_routine_still_runs() {
        let mut r = routine("tell", power(false), Actions::default());
        r.notify = true;
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(step(&mut s, false, at(5, 10, 0), &[r]), vec![run("tell", vec![])]);
    }

    #[test]
    fn state_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation-state.json");
        let s = AutomationState {
            on_ac: Some(true),
            ac_since: Some(at(5, 10, 0)),
            long_ac_done: vec!["long".into()],
            last_schedule: Some(at(6, 8, 0)),
            wifi_seen: true,
            ssid: Some("Office".into()),
            displays: Some(1),
            bluetooth: Some(vec!["Mouse".into()]),
            battery_pct: Some(80),
        };
        save_state(&path, &s).unwrap();
        assert_eq!(load_state(&path), s);
        std::fs::write(&path, "{nope").unwrap();
        assert_eq!(load_state(&path), AutomationState::default());
    }

    #[test]
    fn state_from_before_routines_keeps_the_power_source() {
        let old = r#"{"onAc":true,"acSince":"2026-10-05T10:00:00","longAcFired":true,"lastSchedule":"2026-10-06T08:00:00"}"#;
        let s: AutomationState = serde_json::from_str(old).unwrap();
        assert_eq!(s.on_ac, Some(true));
        assert_eq!(s.last_schedule, Some(at(6, 8, 0)));
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
        assert!(bad(&|r| r.trigger = time(&[], "08:00")));
        assert!(bad(&|r| r.trigger = time(&[7], "08:00")));
        assert!(bad(&|r| r.trigger = time(&[0], "8am")));
        assert!(bad(&|r| r.name = "  ".into()));
        assert!(bad(&|r| r.trigger = wifi(" ", true)));
        assert!(bad(&|r| r.trigger = Trigger::Bluetooth { device: String::new(), connected: true }));
        assert!(bad(&|r| r.trigger = Trigger::BatteryBelow { percent: 2 }));
        assert!(bad(&|r| r.trigger = Trigger::LongAc { days: 0 }));
        assert!(bad(&|r| {
            r.actions = Actions::default();
            r.notify = false;
        }));
        assert!(validate(&[ok.clone(), ok]).is_err());
    }

    #[test]
    fn routines_serialize_for_the_ui() {
        let r = routine("w", wifi("Office", true), Actions { power_mode: Some(PowerMode::Balanced), ..Actions::default() });
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["trigger"], serde_json::json!({ "kind": "wifi", "ssid": "Office", "connected": true }));
        assert_eq!(json["actions"]["powerMode"], "Balanced");
        assert_eq!(json["actions"]["thermal"], serde_json::Value::Null);
        let below = serde_json::to_value(Trigger::BatteryBelow { percent: 20 }).unwrap();
        assert_eq!(below, serde_json::json!({ "kind": "batteryBelow", "percent": 20 }));
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
                    trigger: power(true),
                    actions: thermal(ThermalMode::UltraPerformance),
                    notify: false,
                },
                Routine {
                    id: "long-ac".into(),
                    name: "Plugged in for 5 days".into(),
                    enabled: true,
                    trigger: Trigger::LongAc { days: 5 },
                    actions: battery(BatteryProfile::Storage),
                    notify: false,
                },
                Routine {
                    id: "schedule-0".into(),
                    name: "Remote at 08:00".into(),
                    enabled: false,
                    trigger: time(&[0, 1], "08:00"),
                    actions: battery(BatteryProfile::Campus),
                    notify: false,
                },
            ]
        );
        assert!(validate(&routines).is_ok());
        assert_eq!(migrate(&Automation::default()), vec![]);
    }
}
