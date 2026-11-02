//! The engine: one step per poller tick, pure so it can be tested. A routine starts when its
//! conditions come to hold and ends when one stops holding. It changes the settings only when it
//! starts, so a change made by hand holds until the next start. When several start at once, the one
//! higher in the list wins each setting. A routine that restores puts back what it changed when it
//! ends. An "At" time makes a routine instant: it runs at that moment and never over time.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use super::condition::last_moment;
use super::{Actions, Condition, Readings, Routine, SLOTS, all_hold};
use crate::dell::ThermalMode;
use crate::power_mode::PowerMode;
use crate::profile::BatteryProfile;

/// What the engine remembers between ticks and across restarts (`automation-state.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutomationState {
    pub on_ac: Option<bool>,
    /// Start of the current continuous AC period.
    pub ac_since: Option<NaiveDateTime>,
    /// The last time moment handled; later moments are due.
    pub last_schedule: Option<NaiveDateTime>,
    /// Restoring routines that ran and have not ended yet, oldest first.
    pub active: Vec<Active>,
    /// Routines that started and haven't ended, in the order they started.
    pub running: Vec<String>,
    /// Stopped by hand: they don't start again until their conditions stop holding.
    pub blocked: Vec<String>,
    /// False on a state from before routines ran over time: the next tick adopts what holds.
    pub adopted: bool,
    /// Routines to mark running without changing anything, once their status is known.
    pub adopting: Vec<String>,
    /// ▶ and ■ from the app, for the next tick.
    pub requests: Vec<Request>,
    /// Routines whose conditions or actions changed since the last tick.
    pub edited: Vec<String>,
}

/// Starting or stopping a routine by hand.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum Request {
    Start(String),
    Stop(String),
}

/// What a restoring routine changed (`set`) and what it found there (`before`), per setting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Active {
    pub id: String,
    pub before: Actions,
    pub set: Actions,
}

impl AutomationState {
    /// After the routines change, only moments from now on count: editing never fires a past one.
    /// A routine whose conditions or actions changed starts over on the next tick.
    pub fn rules_changed(&mut self, now: NaiveDateTime, old: &[Routine], new: &[Routine]) {
        self.last_schedule = Some(now);
        for r in new {
            if old.iter().any(|o| o.id == r.id && (o.conditions != r.conditions || o.actions != r.actions))
                && !self.edited.contains(&r.id)
            {
                self.edited.push(r.id.clone());
            }
        }
    }

    pub fn request(&mut self, request: Request) {
        self.requests.push(request);
    }
}

/// Missing or unreadable state starts fresh: the next tick adopts what holds.
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
    /// The settings as they are; `None` where unknown, which is neither recorded nor restored.
    pub current: Actions,
}

impl Inputs {
    pub fn at(now: NaiveDateTime, on_ac: bool) -> Self {
        Self { now, on_ac, battery_pct: None, wifi: None, displays: None, bluetooth: None, current: Actions::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Thermal(ThermalMode),
    Battery(BatteryProfile),
    PowerMode(PowerMode),
}

impl Change {
    fn slot(self) -> usize {
        match self {
            Change::Thermal(_) => 0,
            Change::Battery(_) => 1,
            Change::PowerMode(_) => 2,
        }
    }
}

/// A routine that fired and the changes it won, or one that ended and what it put back.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub id: String,
    pub changes: Vec<Change>,
    pub restored: bool,
}

/// Drops entry `i`. A later entry that changed the same setting inherits what this one found, so
/// overlapping routines unwind to the state before the first. Otherwise, with `current` given, the
/// setting comes back unless it was changed since.
fn release(active: &mut Vec<Active>, i: usize, current: Option<&Actions>) -> Vec<Change> {
    let gone = active.remove(i);
    let mut out = Vec::new();
    for slot in 0..SLOTS {
        let (Some(set), Some(before)) = (gone.set.get(slot), gone.before.get(slot)) else { continue };
        if let Some(later) = active[i..].iter_mut().find(|a| a.set.get(slot).is_some()) {
            later.before.put(before);
        } else if current.is_some_and(|c| c.get(slot) == Some(set)) {
            out.push(before);
        }
    }
    out
}

/// Records what a restoring routine changed against `now`. Firing again keeps what it first found.
fn remember(active: &mut Vec<Active>, id: &str, changes: &[Change], now: &Actions) {
    let i = match active.iter().position(|a| a.id == id) {
        Some(i) => i,
        None => {
            active.push(Active { id: id.into(), ..Active::default() });
            active.len() - 1
        }
    };
    let a = &mut active[i];
    for &c in changes {
        let slot = c.slot();
        match a.before.get(slot).or(now.get(slot)) {
            Some(before) if before != c => {
                a.before.put(before);
                a.set.put(c);
            }
            _ => {
                a.before.clear(slot);
                a.set.clear(slot);
            }
        }
    }
    if a.set.is_empty() {
        active.remove(i);
    }
}

/// Runs over time while its conditions hold, unlike a manual or an instant routine.
fn lasting(r: &Routine) -> bool {
    !r.is_manual() && !r.is_instant()
}

/// The latest moment an enabled instant routine's "At" passed.
fn moment(r: &Routine, now: NaiveDateTime) -> Option<NaiveDateTime> {
    if !r.enabled {
        return None;
    }
    r.conditions.iter().find_map(|c| match c {
        Condition::Time { days, time, until: None } => last_moment(days, time, now),
        _ => None,
    })
}

/// Sets the battery profile for a long stretch on AC.
fn holds_battery_on_ac(r: &Routine) -> bool {
    r.actions.battery.is_some() && r.conditions.iter().any(|c| matches!(c, Condition::LongAc { .. }))
}

/// One engine step. Mutates `state`; the caller persists it when it changed.
pub fn evaluate(state: &mut AutomationState, input: &Inputs, routines: &[Routine]) -> Vec<Run> {
    let now = input.now;
    if state.on_ac != Some(input.on_ac) {
        state.on_ac = Some(input.on_ac);
        state.ac_since = input.on_ac.then_some(now);
    }
    let readings = Readings {
        now,
        on_ac: input.on_ac,
        ac_since: state.ac_since,
        battery_pct: input.battery_pct,
        wifi: input.wifi.as_ref(),
        displays: input.displays,
        bluetooth: input.bluetooth.as_deref(),
    };
    let find = |id: &str| routines.iter().find(|r| r.id == id);

    // Ended this tick, by id: a deleted routine has no place in the list.
    let mut ending: Vec<String> = Vec::new();
    let mut starting = vec![false; routines.len()];
    let stop = |running: &mut Vec<String>, ending: &mut Vec<String>, id: &str| {
        if let Some(k) = running.iter().position(|x| x == id) {
            running.remove(k);
            ending.push(id.to_string());
        }
    };

    // Switched off, deleted or now instant: it ends.
    for id in state.running.clone() {
        if !find(&id).is_some_and(|r| r.enabled && !r.is_instant()) {
            stop(&mut state.running, &mut ending, &id);
        }
    }
    state.blocked.retain(|id| find(id).is_some_and(|r| r.enabled));

    // Edited: it ends, then starts again below if it still should.
    for id in std::mem::take(&mut state.edited) {
        state.blocked.retain(|b| *b != id);
        if state.running.contains(&id) {
            stop(&mut state.running, &mut ending, &id);
            if let Some(i) = routines.iter().position(|r| r.id == id && r.enabled && r.is_manual()) {
                starting[i] = true;
            }
        }
    }

    for request in std::mem::take(&mut state.requests) {
        match request {
            Request::Start(id) => {
                if let Some(i) = routines.iter().position(|r| r.id == id && r.enabled && r.is_manual())
                    && !state.running.contains(&id)
                {
                    starting[i] = true;
                }
            }
            Request::Stop(id) => {
                if state.running.contains(&id) {
                    stop(&mut state.running, &mut ending, &id);
                    if find(&id).is_some_and(lasting) {
                        state.blocked.push(id);
                    }
                }
            }
        }
    }

    // A state from before routines ran over time marks what already holds as running, silently.
    if !state.adopted {
        state.adopted = true;
        state.adopting = routines.iter().filter(|r| r.enabled && lasting(r)).map(|r| r.id.clone()).collect();
    }
    let running = &mut state.running;
    state.adopting.retain(|id| {
        let Some(r) = find(id).filter(|r| r.enabled && lasting(r)) else { return false };
        match all_hold(&r.conditions, &readings) {
            None => true,
            Some(holds) => {
                if holds && !running.contains(id) {
                    running.push(id.clone());
                }
                false
            }
        }
    });

    // After a gap only the latest missed moment runs.
    let mut due = None;
    let latest = routines.iter().filter_map(|r| moment(r, now)).max();
    match state.last_schedule {
        None => state.last_schedule = Some(now),
        Some(last) => {
            if let Some(latest) = latest
                && latest > last
            {
                state.last_schedule = Some(latest);
                due = Some(latest);
            }
        }
    }

    for (i, r) in routines.iter().enumerate() {
        if !r.enabled || r.is_manual() || state.adopting.contains(&r.id) {
            continue;
        }
        let holds = all_hold(&r.conditions, &readings);
        if r.is_instant() {
            starting[i] = due.is_some() && moment(r, now) == due && holds == Some(true);
            continue;
        }
        match holds {
            Some(true) => starting[i] = !state.running.contains(&r.id) && !state.blocked.contains(&r.id),
            Some(false) => {
                state.blocked.retain(|b| *b != r.id);
                stop(&mut state.running, &mut ending, &r.id);
            }
            None => {}
        }
    }

    // Leaving a long stretch on AC without restoring hands the profile back to the time routines.
    let back_to_schedule = ending.iter().filter_map(|id| find(id)).any(|r| !r.restore && holds_battery_on_ac(r));

    // Forget what to restore for routines no longer restoring. One that ends this tick, deleted or
    // switched off included, still restores.
    let keep = |id: &String| {
        if ending.contains(id) {
            find(id).is_none_or(|r| r.restore)
        } else {
            find(id).is_some_and(|r| r.enabled && r.restore && !r.is_instant())
        }
    };
    let mut k = 0;
    while k < state.active.len() {
        if keep(&state.active[k].id) {
            k += 1;
        } else {
            release(&mut state.active, k, None);
        }
    }

    // Routines that ended, newest first; `pending` is what the settings are about to be.
    let mut restores: Vec<(String, Vec<Change>)> = Vec::new();
    let mut pending = Actions::default();
    for k in (0..state.active.len()).rev() {
        let id = state.active[k].id.clone();
        if !ending.contains(&id) {
            continue;
        }
        let changes = release(&mut state.active, k, Some(&pending.or(&input.current)));
        for &c in &changes {
            for (_, earlier) in &mut restores {
                earlier.retain(|e| e.slot() != c.slot());
            }
            pending.put(c);
        }
        restores.push((id, changes));
    }

    for (r, _) in routines.iter().zip(&starting).filter(|(r, s)| **s && !r.is_instant()) {
        state.running.push(r.id.clone());
    }

    // While a long-AC routine holds the battery profile, instant routines leave it alone.
    let hold = state.running.iter().filter_map(|id| find(id)).any(holds_battery_on_ac);
    let mut claimed = [false; SLOTS];
    let mut runs = Vec::new();
    for (r, _) in routines.iter().zip(&starting).filter(|(_, s)| **s) {
        let held = hold && r.is_instant();
        let changes: Vec<Change> = (0..SLOTS)
            .filter_map(|slot| r.actions.get(slot))
            .filter(|c| !(held && c.slot() == 1) && !std::mem::replace(&mut claimed[c.slot()], true))
            .collect();
        if r.restore && !r.is_instant() {
            remember(&mut state.active, &r.id, &changes, &pending.or(&input.current));
        }
        runs.push(Run { id: r.id.clone(), changes, restored: false });
    }

    // A routine that started wins over one putting a setting back.
    for (id, changes) in restores.into_iter().rev() {
        let changes: Vec<Change> =
            changes.into_iter().filter(|c| !std::mem::replace(&mut claimed[c.slot()], true)).collect();
        if !changes.is_empty() {
            runs.push(Run { id, changes, restored: true });
        }
    }

    if back_to_schedule && !claimed[1] {
        let latest = routines
            .iter()
            .filter(|r| r.actions.battery.is_some() && r.is_instant())
            .filter(|r| all_hold(&r.conditions, &readings) == Some(true))
            .filter_map(|r| moment(r, now).map(|m| (m, r)))
            .max_by_key(|(m, _)| *m);
        if let Some((_, r)) = latest
            && let Some(profile) = r.actions.battery
        {
            runs.push(Run { id: r.id.clone(), changes: vec![Change::Battery(profile)], restored: false });
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

    fn routine(id: &str, trigger: Condition, actions: Actions) -> Routine {
        Routine { id: id.into(), name: id.into(), enabled: true, conditions: vec![trigger], actions, notify: true, restore: false }
    }

    fn restoring(id: &str, trigger: Condition, actions: Actions) -> Routine {
        Routine { restore: true, ..routine(id, trigger, actions) }
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

    const EVERY_DAY: &[u8] = &[0, 1, 2, 3, 4, 5, 6];

    fn plug_rules() -> Vec<Routine> {
        vec![
            routine("ac", power(true), thermal(ThermalMode::UltraPerformance)),
            routine("bat", power(false), thermal(ThermalMode::Quiet)),
        ]
    }

    fn seen(on_ac: bool, now: NaiveDateTime) -> AutomationState {
        AutomationState {
            on_ac: Some(on_ac),
            ac_since: on_ac.then_some(now),
            last_schedule: Some(now),
            adopted: true,
            ..Default::default()
        }
    }

    /// A state from before routines ran over time: the first tick adopts.
    fn fresh(on_ac: bool, now: NaiveDateTime) -> AutomationState {
        AutomationState { adopted: false, ..seen(on_ac, now) }
    }

    fn run(id: &str, changes: Vec<Change>) -> Run {
        Run { id: id.into(), changes, restored: false }
    }

    fn restored(id: &str, changes: Vec<Change>) -> Run {
        Run { id: id.into(), changes, restored: true }
    }

    /// The settings as they are now.
    fn now_at(mode: ThermalMode) -> Actions {
        Actions { thermal: Some(mode), battery: Some(BatteryProfile::Home), power_mode: Some(PowerMode::Balanced) }
    }

    fn step(s: &mut AutomationState, on_ac: bool, now: NaiveDateTime, rules: &[Routine]) -> Vec<Run> {
        evaluate(s, &Inputs::at(now, on_ac), rules)
    }

    #[test]
    fn the_first_tick_adopts_without_changing_anything() {
        let mut s = AutomationState::default();
        assert_eq!(step(&mut s, true, at(5, 10, 0), &plug_rules()), vec![]);
        assert_eq!(s.on_ac, Some(true));
        assert_eq!(s.ac_since, Some(at(5, 10, 0)));
        assert_eq!(s.last_schedule, Some(at(5, 10, 0)));
        assert_eq!(s.running, vec!["ac".to_string()]);
        // Unplugged later: the AC routine ends, the battery one starts.
        assert_eq!(step(&mut s, false, at(5, 11, 0), &plug_rules()), vec![run("bat", vec![Change::Thermal(ThermalMode::Quiet)])]);
        assert_eq!(s.running, vec!["bat".to_string()]);
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
    fn a_routine_runs_once_while_its_conditions_hold() {
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(step(&mut s, false, at(5, 10, 0), &plug_rules()).len(), 1);
        assert_eq!(step(&mut s, false, at(5, 10, 1), &plug_rules()), vec![]);
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
        s.rules_changed(at(5, 10, 0), &rules, &rules);
        assert_eq!(step(&mut s, false, at(5, 10, 1), &rules), vec![]);
    }

    fn long_ac(days: u32) -> Routine {
        routine("long", Condition::LongAc { days }, battery(BatteryProfile::Storage))
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
        assert_eq!(s.running, vec!["long".to_string()]);
        assert_eq!(step(&mut s, true, at(8, 8, 0), &rules), vec![run("am", vec![])]);
    }

    #[test]
    fn unplugging_after_long_ac_restores_the_time_routine_profile() {
        let rules = vec![long_ac(3), routine("am", time(EVERY_DAY, "08:00"), battery(BatteryProfile::Home))];
        let mut s = seen(true, at(5, 7, 0));
        step(&mut s, true, at(8, 7, 0), &rules);
        step(&mut s, true, at(8, 11, 0), &rules);
        assert_eq!(step(&mut s, false, at(8, 12, 0), &rules), vec![run("am", vec![Change::Battery(BatteryProfile::Home)])]);
        assert!(s.running.is_empty());
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

    fn wifi(ssid: &str, connected: bool) -> Condition {
        Condition::Wifi { ssid: ssid.into(), connected }
    }

    fn on_wifi(now: NaiveDateTime, ssid: Option<&str>) -> Inputs {
        Inputs { wifi: Some(ssid.map(String::from)), ..Inputs::at(now, true) }
    }

    #[test]
    fn wifi_routines_follow_the_network() {
        let rules = vec![
            routine("join", wifi("Office", true), thermal(ThermalMode::Optimized)),
            routine("leave", wifi("Office", false), thermal(ThermalMode::Quiet)),
        ];
        let mut s = fresh(true, at(5, 9, 0));
        // Adopted: away from the office, "leave" already holds.
        assert_eq!(evaluate(&mut s, &on_wifi(at(5, 9, 0), Some("Home")), &rules), vec![]);
        assert_eq!(s.running, vec!["leave".to_string()]);
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
    fn unknown_input_changes_nothing() {
        let rules = vec![routine("join", wifi("Office", true), thermal(ThermalMode::Quiet))];
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(evaluate(&mut s, &on_wifi(at(5, 9, 0), Some("Office")), &rules).len(), 1);
        // Wi-Fi not read this tick: still running, nothing ends.
        assert_eq!(step(&mut s, true, at(5, 9, 1), &rules), vec![]);
        assert_eq!(s.running, vec!["join".to_string()]);
        // A routine whose input was never read doesn't start either.
        let mut t = seen(true, at(5, 9, 0));
        assert_eq!(step(&mut t, true, at(5, 9, 1), &rules), vec![]);
        assert!(t.running.is_empty());
    }

    #[test]
    fn display_routines_run_on_the_first_and_last_external_display() {
        let rules = vec![
            routine("dock", Condition::Display { connected: true }, thermal(ThermalMode::UltraPerformance)),
            routine("undock", Condition::Display { connected: false }, thermal(ThermalMode::Optimized)),
        ];
        let displays = |now, n| Inputs { displays: Some(n), ..Inputs::at(now, true) };
        let mut s = fresh(true, at(5, 9, 0));
        assert_eq!(evaluate(&mut s, &displays(at(5, 9, 0), 0), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &displays(at(5, 9, 1), 1), &rules), vec![run("dock", vec![Change::Thermal(ThermalMode::UltraPerformance)])]);
        assert_eq!(evaluate(&mut s, &displays(at(5, 9, 2), 2), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &displays(at(5, 9, 3), 0), &rules), vec![run("undock", vec![Change::Thermal(ThermalMode::Optimized)])]);
    }

    #[test]
    fn bluetooth_routines_follow_one_device() {
        let device = |connected| Condition::Bluetooth { device: "WH-1000XM5".into(), connected };
        let rules = vec![
            routine("on", device(true), thermal(ThermalMode::Quiet)),
            routine("off", device(false), thermal(ThermalMode::Optimized)),
        ];
        let bt = |now, names: &[&str]| Inputs {
            bluetooth: Some(names.iter().map(|n| n.to_string()).collect()),
            ..Inputs::at(now, true)
        };
        let mut s = fresh(true, at(5, 9, 0));
        assert_eq!(evaluate(&mut s, &bt(at(5, 9, 0), &["Mouse"]), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &bt(at(5, 9, 1), &["Mouse", "WH-1000XM5"]), &rules), vec![run("on", vec![Change::Thermal(ThermalMode::Quiet)])]);
        assert_eq!(evaluate(&mut s, &bt(at(5, 9, 2), &["WH-1000XM5"]), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &bt(at(5, 9, 3), &[]), &rules), vec![run("off", vec![Change::Thermal(ThermalMode::Optimized)])]);
    }

    #[test]
    fn battery_below_runs_when_the_level_drops_past_it() {
        let rules = vec![routine("low", Condition::BatteryBelow { percent: 20 }, thermal(ThermalMode::Quiet))];
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

    fn wifi_now(now: NaiveDateTime, ssid: Option<&str>, current: ThermalMode) -> Inputs {
        Inputs { current: now_at(current), ..on_wifi(now, ssid) }
    }

    #[test]
    fn a_restoring_routine_puts_back_what_it_changed_when_it_ends() {
        let rules = vec![restoring("join", wifi("Office", true), thermal(ThermalMode::UltraPerformance))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), None, ThermalMode::Optimized), &rules);
        assert_eq!(
            evaluate(&mut s, &wifi_now(at(5, 9, 1), Some("Office"), ThermalMode::Optimized), &rules),
            vec![run("join", vec![Change::Thermal(ThermalMode::UltraPerformance)])]
        );
        assert_eq!(evaluate(&mut s, &wifi_now(at(5, 9, 2), Some("Office"), ThermalMode::UltraPerformance), &rules), vec![]);
        assert_eq!(
            evaluate(&mut s, &wifi_now(at(5, 9, 3), None, ThermalMode::UltraPerformance), &rules),
            vec![restored("join", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
        assert!(s.active.is_empty());
    }

    #[test]
    fn without_restore_nothing_comes_back() {
        let rules = vec![routine("join", wifi("Office", true), thermal(ThermalMode::UltraPerformance))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), None, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &wifi_now(at(5, 9, 1), Some("Office"), ThermalMode::Optimized), &rules);
        assert_eq!(evaluate(&mut s, &wifi_now(at(5, 9, 2), None, ThermalMode::UltraPerformance), &rules), vec![]);
        assert!(s.active.is_empty());
    }

    #[test]
    fn a_setting_changed_by_hand_is_not_restored() {
        let rules = vec![restoring(
            "join",
            wifi("Office", true),
            Actions { thermal: Some(ThermalMode::UltraPerformance), power_mode: Some(PowerMode::BestPerformance), ..Actions::default() },
        )];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), None, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &wifi_now(at(5, 9, 1), Some("Office"), ThermalMode::Optimized), &rules);
        // Thermal changed by hand to Quiet; the power mode is still what the routine set.
        let mut leave = wifi_now(at(5, 9, 2), None, ThermalMode::Quiet);
        leave.current.power_mode = Some(PowerMode::BestPerformance);
        assert_eq!(evaluate(&mut s, &leave, &rules), vec![restored("join", vec![Change::PowerMode(PowerMode::Balanced)])]);
    }

    #[test]
    fn nothing_is_restored_when_the_setting_was_already_there() {
        let rules = vec![restoring("join", wifi("Office", true), thermal(ThermalMode::Optimized))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), None, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &wifi_now(at(5, 9, 1), Some("Office"), ThermalMode::Optimized), &rules);
        assert!(s.active.is_empty());
        assert_eq!(evaluate(&mut s, &wifi_now(at(5, 9, 2), None, ThermalMode::Optimized), &rules), vec![]);
    }

    fn overlapping() -> Vec<Routine> {
        vec![
            restoring("office", wifi("Office", true), thermal(ThermalMode::UltraPerformance)),
            restoring("dock", Condition::Display { connected: true }, thermal(ThermalMode::Cool)),
        ]
    }

    fn office_and_dock(now: NaiveDateTime, office: bool, displays: u32, current: ThermalMode) -> Inputs {
        Inputs { displays: Some(displays), ..wifi_now(now, office.then_some("Office"), current) }
    }

    #[test]
    fn overlapping_routines_restore_the_state_from_before_the_first() {
        let rules = overlapping();
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &office_and_dock(at(5, 9, 0), false, 0, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &office_and_dock(at(5, 9, 1), true, 0, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &office_and_dock(at(5, 9, 2), true, 1, ThermalMode::UltraPerformance), &rules);
        // The first one ends while the second still holds the mode: nothing changes yet.
        assert_eq!(evaluate(&mut s, &office_and_dock(at(5, 9, 3), false, 1, ThermalMode::Cool), &rules), vec![]);
        assert_eq!(
            evaluate(&mut s, &office_and_dock(at(5, 9, 4), false, 0, ThermalMode::Cool), &rules),
            vec![restored("dock", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
    }

    #[test]
    fn overlapping_routines_ending_in_reverse_unwind_one_by_one() {
        let rules = overlapping();
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &office_and_dock(at(5, 9, 0), false, 0, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &office_and_dock(at(5, 9, 1), true, 0, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &office_and_dock(at(5, 9, 2), true, 1, ThermalMode::UltraPerformance), &rules);
        assert_eq!(
            evaluate(&mut s, &office_and_dock(at(5, 9, 3), true, 0, ThermalMode::Cool), &rules),
            vec![restored("dock", vec![Change::Thermal(ThermalMode::UltraPerformance)])]
        );
        assert_eq!(
            evaluate(&mut s, &office_and_dock(at(5, 9, 4), false, 0, ThermalMode::UltraPerformance), &rules),
            vec![restored("office", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
    }

    #[test]
    fn a_routine_firing_as_another_ends_wins_and_restores_the_original_later() {
        let rules = vec![
            restoring("office", wifi("Office", true), thermal(ThermalMode::UltraPerformance)),
            restoring("home", wifi("Home", true), thermal(ThermalMode::Quiet)),
        ];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), None, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &wifi_now(at(5, 9, 1), Some("Office"), ThermalMode::Optimized), &rules);
        assert_eq!(
            evaluate(&mut s, &wifi_now(at(5, 9, 2), Some("Home"), ThermalMode::UltraPerformance), &rules),
            vec![run("home", vec![Change::Thermal(ThermalMode::Quiet)])]
        );
        assert_eq!(
            evaluate(&mut s, &wifi_now(at(5, 9, 3), None, ThermalMode::Quiet), &rules),
            vec![restored("home", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
    }

    #[test]
    fn battery_below_ends_when_the_level_is_back() {
        let rules = vec![restoring("low", Condition::BatteryBelow { percent: 20 }, thermal(ThermalMode::Quiet))];
        let pct = |now, p, mode| Inputs { battery_pct: Some(p), current: now_at(mode), ..Inputs::at(now, false) };
        let mut s = seen(false, at(5, 9, 0));
        evaluate(&mut s, &pct(at(5, 9, 0), 21.0, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &pct(at(5, 9, 1), 19.0, ThermalMode::Optimized), &rules);
        assert_eq!(evaluate(&mut s, &pct(at(5, 9, 2), 19.5, ThermalMode::Quiet), &rules), vec![]);
        assert_eq!(
            evaluate(&mut s, &pct(at(5, 9, 3), 20.0, ThermalMode::Quiet), &rules),
            vec![restored("low", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
    }

    #[test]
    fn long_ac_ends_when_unplugged() {
        let rules = vec![
            Routine { restore: true, ..long_ac(3) },
            routine("am", time(EVERY_DAY, "08:00"), battery(BatteryProfile::Campus)),
        ];
        let tick = |now, on_ac| Inputs { current: now_at(ThermalMode::Optimized), ..Inputs::at(now, on_ac) };
        let mut s = seen(true, at(5, 7, 0));
        evaluate(&mut s, &tick(at(5, 7, 0), true), &rules);
        evaluate(&mut s, &tick(at(8, 7, 0), true), &rules);
        let mut unplug = tick(at(8, 7, 30), false);
        unplug.current.battery = Some(BatteryProfile::Storage);
        // Back to what it was, not to the time routine's profile.
        assert_eq!(evaluate(&mut s, &unplug, &rules), vec![restored("long", vec![Change::Battery(BatteryProfile::Home)])]);
    }

    #[test]
    fn plugging_in_ends_an_on_battery_routine() {
        let rules = vec![restoring("bat", power(false), thermal(ThermalMode::Quiet))];
        let tick = |now, on_ac, mode| Inputs { current: now_at(mode), ..Inputs::at(now, on_ac) };
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &tick(at(5, 9, 1), false, ThermalMode::UltraPerformance), &rules);
        assert_eq!(
            evaluate(&mut s, &tick(at(5, 9, 2), true, ThermalMode::Quiet), &rules),
            vec![restored("bat", vec![Change::Thermal(ThermalMode::UltraPerformance)])]
        );
    }

    #[test]
    fn time_routines_have_no_end_to_restore_at() {
        let rules = vec![restoring("am", time(EVERY_DAY, "08:00"), thermal(ThermalMode::Quiet))];
        let mut s = seen(true, at(5, 7, 0));
        let tick = Inputs { current: now_at(ThermalMode::Optimized), ..Inputs::at(at(5, 8, 0), true) };
        assert_eq!(evaluate(&mut s, &tick, &rules), vec![run("am", vec![Change::Thermal(ThermalMode::Quiet)])]);
        assert!(s.active.is_empty());
    }

    #[test]
    fn switching_restore_off_forgets_what_to_restore() {
        let mut rules = vec![restoring("join", wifi("Office", true), thermal(ThermalMode::UltraPerformance))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), None, ThermalMode::Optimized), &rules);
        evaluate(&mut s, &wifi_now(at(5, 9, 1), Some("Office"), ThermalMode::Optimized), &rules);
        rules[0].restore = false;
        evaluate(&mut s, &wifi_now(at(5, 9, 2), Some("Office"), ThermalMode::UltraPerformance), &rules);
        assert!(s.active.is_empty());
        rules[0].restore = true;
        assert_eq!(evaluate(&mut s, &wifi_now(at(5, 9, 3), None, ThermalMode::UltraPerformance), &rules), vec![]);
    }

    #[test]
    fn state_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("automation-state.json");
        let s = AutomationState {
            on_ac: Some(true),
            ac_since: Some(at(5, 10, 0)),
            last_schedule: Some(at(6, 8, 0)),
            running: vec!["long".into(), "join".into()],
            blocked: vec!["dock".into()],
            adopted: true,
            adopting: vec!["bt".into()],
            requests: vec![Request::Start("focus".into()), Request::Stop("office".into())],
            edited: vec!["office".into()],
            active: vec![Active { id: "join".into(), before: thermal(ThermalMode::Quiet), set: thermal(ThermalMode::Cool) }],
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
    fn both_conditions_must_hold() {
        let office = Routine {
            restore: true,
            conditions: vec![wifi("Office", true), power(true)],
            ..routine("office", power(true), thermal(ThermalMode::Quiet))
        };
        let rules = vec![office];
        let tick = |now, ssid: Option<&str>, on_ac| Inputs {
            wifi: Some(ssid.map(String::from)),
            current: now_at(ThermalMode::Optimized),
            ..Inputs::at(now, on_ac)
        };
        let mut s = seen(false, at(5, 9, 0));
        assert_eq!(evaluate(&mut s, &tick(at(5, 9, 1), Some("Office"), false), &rules), vec![]);
        assert_eq!(
            evaluate(&mut s, &tick(at(5, 9, 2), Some("Office"), true), &rules),
            vec![run("office", vec![Change::Thermal(ThermalMode::Quiet)])]
        );
        let mut unplug = tick(at(5, 9, 3), Some("Office"), false);
        unplug.current.thermal = Some(ThermalMode::Quiet);
        assert_eq!(evaluate(&mut s, &unplug, &rules), vec![restored("office", vec![Change::Thermal(ThermalMode::Optimized)])]);
        assert!(s.running.is_empty());
    }

    #[test]
    fn between_runs_for_its_window() {
        let work = Condition::Time { days: vec![0], time: "09:00".into(), until: Some("17:00".into()) };
        let rules = vec![restoring("work", work, thermal(ThermalMode::Quiet))];
        let tick = |now, mode| Inputs { current: now_at(mode), ..Inputs::at(now, true) };
        let mut s = seen(true, at(5, 8, 0));
        assert_eq!(evaluate(&mut s, &tick(at(5, 8, 59), ThermalMode::Optimized), &rules), vec![]);
        assert_eq!(
            evaluate(&mut s, &tick(at(5, 9, 0), ThermalMode::Optimized), &rules),
            vec![run("work", vec![Change::Thermal(ThermalMode::Quiet)])]
        );
        assert_eq!(evaluate(&mut s, &tick(at(5, 12, 0), ThermalMode::Quiet), &rules), vec![]);
        assert_eq!(
            evaluate(&mut s, &tick(at(5, 17, 0), ThermalMode::Quiet), &rules),
            vec![restored("work", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
    }

    #[test]
    fn at_needs_the_other_conditions() {
        let am = Routine {
            conditions: vec![time(EVERY_DAY, "08:00"), power(true)],
            ..routine("am", power(true), battery(BatteryProfile::Home))
        };
        let rules = vec![am];
        let mut s = seen(false, at(5, 7, 0));
        assert_eq!(step(&mut s, false, at(5, 8, 0), &rules), vec![]);
        let mut t = seen(false, at(5, 7, 0));
        assert_eq!(step(&mut t, true, at(5, 8, 0), &rules), vec![run("am", vec![Change::Battery(BatteryProfile::Home)])]);
        assert!(t.running.is_empty());
    }

    #[test]
    fn upgrade_adopts_silently() {
        let old = r#"{"onAc":true,"acSince":"2026-10-01T10:00:00","longAcDone":["long"],"lastSchedule":"2026-10-05T08:00:00","wifiSeen":true,"ssid":"Office"}"#;
        let mut s: AutomationState = serde_json::from_str(old).unwrap();
        let rules = vec![long_ac(3), routine("office", wifi("Office", true), thermal(ThermalMode::Quiet))];
        assert_eq!(step(&mut s, true, at(5, 9, 0), &rules), vec![]);
        assert_eq!(s.running, vec!["long".to_string()]);
        assert_eq!(evaluate(&mut s, &on_wifi(at(5, 9, 1), Some("Office")), &rules), vec![]);
        assert_eq!(s.running, vec!["long".to_string(), "office".to_string()]);
        assert!(s.adopting.is_empty());
    }

    #[test]
    fn a_new_routine_starts_when_its_conditions_hold() {
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(step(&mut s, true, at(5, 9, 1), &[]), vec![]);
        let rules = vec![routine("ac", power(true), thermal(ThermalMode::UltraPerformance))];
        assert_eq!(step(&mut s, true, at(5, 9, 2), &rules), vec![run("ac", vec![Change::Thermal(ThermalMode::UltraPerformance)])]);
    }

    fn manual(id: &str, actions: Actions) -> Routine {
        Routine { conditions: vec![], ..routine(id, power(true), actions) }
    }

    fn by_hand(now: NaiveDateTime, mode: ThermalMode) -> Inputs {
        Inputs { current: now_at(mode), ..Inputs::at(now, true) }
    }

    #[test]
    fn requests_serialize_with_the_id() {
        let json = serde_json::to_value(Request::Start("focus".into())).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "start", "id": "focus" }));
    }

    #[test]
    fn manual_routine_starts_and_stops() {
        let rules = vec![Routine { restore: true, ..manual("focus", thermal(ThermalMode::Quiet)) }];
        let mut s = seen(true, at(5, 9, 0));
        assert_eq!(evaluate(&mut s, &by_hand(at(5, 9, 1), ThermalMode::Optimized), &rules), vec![]);
        s.request(Request::Start("focus".into()));
        assert_eq!(
            evaluate(&mut s, &by_hand(at(5, 9, 2), ThermalMode::Optimized), &rules),
            vec![run("focus", vec![Change::Thermal(ThermalMode::Quiet)])]
        );
        assert_eq!(s.running, vec!["focus".to_string()]);
        assert_eq!(evaluate(&mut s, &by_hand(at(5, 9, 3), ThermalMode::Quiet), &rules), vec![]);
        s.request(Request::Stop("focus".into()));
        assert_eq!(
            evaluate(&mut s, &by_hand(at(5, 9, 4), ThermalMode::Quiet), &rules),
            vec![restored("focus", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
        assert!(s.running.is_empty());
        assert!(s.requests.is_empty());
    }

    #[test]
    fn a_manual_routine_without_restore_just_stops() {
        let rules = vec![manual("focus", thermal(ThermalMode::Quiet))];
        let mut s = seen(true, at(5, 9, 0));
        s.request(Request::Start("focus".into()));
        assert_eq!(step(&mut s, true, at(5, 9, 1), &rules).len(), 1);
        s.request(Request::Stop("focus".into()));
        assert_eq!(step(&mut s, true, at(5, 9, 2), &rules), vec![]);
        assert!(s.running.is_empty());
    }

    #[test]
    fn start_twice_runs_once() {
        let rules = vec![manual("focus", thermal(ThermalMode::Quiet))];
        let mut s = seen(true, at(5, 9, 0));
        s.request(Request::Start("focus".into()));
        s.request(Request::Start("focus".into()));
        assert_eq!(step(&mut s, true, at(5, 9, 1), &rules).len(), 1);
        s.request(Request::Start("focus".into()));
        assert_eq!(step(&mut s, true, at(5, 9, 2), &rules), vec![]);
        assert_eq!(s.running, vec!["focus".to_string()]);
    }

    #[test]
    fn start_ignores_automatic_switched_off_and_unknown_routines() {
        let mut off = manual("off", thermal(ThermalMode::Cool));
        off.enabled = false;
        let rules = vec![routine("bat", power(false), thermal(ThermalMode::Quiet)), off];
        let mut s = seen(true, at(5, 9, 0));
        for id in ["bat", "off", "gone"] {
            s.request(Request::Start(id.into()));
        }
        assert_eq!(step(&mut s, true, at(5, 9, 1), &rules), vec![]);
        assert!(s.running.is_empty());
    }

    #[test]
    fn stop_blocks_until_the_conditions_break() {
        let rules = vec![restoring("office", wifi("Office", true), thermal(ThermalMode::UltraPerformance))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), Some("Office"), ThermalMode::Optimized), &rules);
        s.request(Request::Stop("office".into()));
        assert_eq!(
            evaluate(&mut s, &wifi_now(at(5, 9, 1), Some("Office"), ThermalMode::UltraPerformance), &rules),
            vec![restored("office", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
        assert_eq!(s.blocked, vec!["office".to_string()]);
        assert_eq!(evaluate(&mut s, &wifi_now(at(5, 9, 2), Some("Office"), ThermalMode::Optimized), &rules), vec![]);
        assert_eq!(evaluate(&mut s, &wifi_now(at(5, 9, 3), Some("Home"), ThermalMode::Optimized), &rules), vec![]);
        assert!(s.blocked.is_empty());
        assert_eq!(
            evaluate(&mut s, &wifi_now(at(5, 9, 4), Some("Office"), ThermalMode::Optimized), &rules),
            vec![run("office", vec![Change::Thermal(ThermalMode::UltraPerformance)])]
        );
    }

    #[test]
    fn editing_a_running_routine_restarts_it() {
        let quiet_and_efficient =
            Actions { thermal: Some(ThermalMode::Quiet), power_mode: Some(PowerMode::BestEfficiency), ..Actions::default() };
        let old = vec![restoring("office", wifi("Office", true), quiet_and_efficient)];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), Some("Office"), ThermalMode::Optimized), &old);
        let new = vec![restoring("office", wifi("Office", true), thermal(ThermalMode::Cool))];
        s.rules_changed(at(5, 9, 1), &old, &new);
        let mut tick = wifi_now(at(5, 9, 2), Some("Office"), ThermalMode::Quiet);
        tick.current.power_mode = Some(PowerMode::BestEfficiency);
        assert_eq!(
            evaluate(&mut s, &tick, &new),
            vec![
                run("office", vec![Change::Thermal(ThermalMode::Cool)]),
                restored("office", vec![Change::PowerMode(PowerMode::Balanced)]),
            ]
        );
        assert!(s.edited.is_empty());
        // It still puts back what was there before it first started.
        assert_eq!(
            evaluate(&mut s, &wifi_now(at(5, 9, 3), Some("Home"), ThermalMode::Cool), &new),
            vec![restored("office", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
    }

    #[test]
    fn editing_a_running_manual_routine_restarts_it() {
        let old = vec![manual("focus", thermal(ThermalMode::Quiet))];
        let mut s = seen(true, at(5, 9, 0));
        s.request(Request::Start("focus".into()));
        step(&mut s, true, at(5, 9, 1), &old);
        let new = vec![manual("focus", thermal(ThermalMode::Cool))];
        s.rules_changed(at(5, 9, 2), &old, &new);
        assert_eq!(step(&mut s, true, at(5, 9, 3), &new), vec![run("focus", vec![Change::Thermal(ThermalMode::Cool)])]);
        assert_eq!(s.running, vec!["focus".to_string()]);
    }

    #[test]
    fn renaming_does_not_restart() {
        let old = vec![restoring("office", wifi("Office", true), thermal(ThermalMode::Quiet))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), Some("Office"), ThermalMode::Optimized), &old);
        let mut new = old.clone();
        new[0].name = "Work".into();
        new[0].notify = false;
        s.rules_changed(at(5, 9, 1), &old, &new);
        assert!(s.edited.is_empty());
        assert_eq!(evaluate(&mut s, &wifi_now(at(5, 9, 2), Some("Office"), ThermalMode::Quiet), &new), vec![]);
    }

    #[test]
    fn editing_clears_a_stop() {
        let old = vec![routine("office", wifi("Office", true), thermal(ThermalMode::Quiet))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &on_wifi(at(5, 9, 0), Some("Office")), &old);
        s.request(Request::Stop("office".into()));
        evaluate(&mut s, &on_wifi(at(5, 9, 1), Some("Office")), &old);
        let new = vec![routine("office", wifi("Office", true), thermal(ThermalMode::Cool))];
        s.rules_changed(at(5, 9, 2), &old, &new);
        assert_eq!(
            evaluate(&mut s, &on_wifi(at(5, 9, 3), Some("Office")), &new),
            vec![run("office", vec![Change::Thermal(ThermalMode::Cool)])]
        );
    }

    #[test]
    fn switching_off_a_running_routine_restores() {
        let mut rules = vec![restoring("office", wifi("Office", true), thermal(ThermalMode::UltraPerformance))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), Some("Office"), ThermalMode::Optimized), &rules);
        rules[0].enabled = false;
        assert_eq!(
            evaluate(&mut s, &wifi_now(at(5, 9, 1), Some("Office"), ThermalMode::UltraPerformance), &rules),
            vec![restored("office", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
        assert!(s.running.is_empty());
        assert!(s.active.is_empty());
    }

    #[test]
    fn deleting_a_running_routine_restores() {
        let rules = vec![restoring("office", wifi("Office", true), thermal(ThermalMode::UltraPerformance))];
        let mut s = seen(true, at(5, 9, 0));
        evaluate(&mut s, &wifi_now(at(5, 9, 0), Some("Office"), ThermalMode::Optimized), &rules);
        assert_eq!(
            evaluate(&mut s, &wifi_now(at(5, 9, 1), Some("Office"), ThermalMode::UltraPerformance), &[]),
            vec![restored("office", vec![Change::Thermal(ThermalMode::Optimized)])]
        );
        assert!(s.running.is_empty());
    }
}
