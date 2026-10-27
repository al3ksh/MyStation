//! Runs the routines on every poller tick and applies what they decide.

use std::path::PathBuf;
use std::sync::Mutex;

use mystation_core::automation::{Actions, Change, Inputs, Routine, evaluate, save_state};
use mystation_core::dell::ThermalMode;
use mystation_core::power_mode::PowerMode;
use mystation_core::profile::{ActiveProfile, BatteryProfile};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::commands;
use crate::platform;
use crate::state::{Core, data_dir};
use crate::sync::LockExt;

pub fn state_path() -> PathBuf {
    data_dir().join("automation-state.json")
}

/// Persist the engine state; a failed write only costs a repeat after a restart.
pub fn persist(state: &mystation_core::automation::AutomationState) {
    if let Err(e) = save_state(&state_path(), state) {
        eprintln!("automation: could not save state: {e}");
    }
}

/// One engine step. Writes run on a helper thread: cctk takes seconds. True when a routine
/// sets the Windows power mode itself.
pub fn tick(app: &AppHandle, mut inputs: Inputs) -> bool {
    let core = app.state::<Core>();
    let now = core.state();
    // What restoring routines record and compare against; a custom profile is no known one.
    inputs.current = Actions {
        thermal: now.thermal,
        battery: match now.active_profile {
            Some(ActiveProfile::Known { profile }) => Some(profile),
            _ => None,
        },
        power_mode: now.power_mode,
    };
    let routines = core.config.lock_ok().routines.clone();
    let runs = {
        let mut state = core.automation.lock_ok();
        let before = state.clone();
        let runs = evaluate(&mut state, &inputs, &routines);
        if *state != before {
            persist(&state);
        }
        runs
    };
    if runs.is_empty() {
        return false;
    }
    let sets_power_mode = runs.iter().flat_map(|r| &r.changes).any(|c| matches!(c, Change::PowerMode(_)));
    let app = app.clone();
    std::thread::spawn(move || {
        // Automatic writes never overlap one another.
        static WRITING: Mutex<()> = Mutex::new(());
        let _guard = WRITING.lock_ok();
        let mut switched: Vec<Vec<&'static str>> = vec![Vec::new(); runs.len()];
        // Thermal first: with the modes matched it also sets the power mode, which a routine's
        // own power mode then overrides.
        for rank in 0..3 {
            if rank == 2 && sets_power_mode {
                // Windows brings back its own mode on a plug change; let it land first.
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
            for (run, names) in runs.iter().zip(switched.iter_mut()) {
                for &change in run.changes.iter().filter(|c| order(**c) == rank) {
                    if let Some(name) = apply(&app, change, routine(&routines, &run.id)) {
                        names.push(name);
                    }
                }
            }
        }
        crate::tray::refresh(&app, &app.state::<Core>().state(), None);
        for (run, names) in runs.iter().zip(switched) {
            if let Some(r) = routine(&routines, &run.id)
                && r.notify
            {
                if run.restored {
                    // Nothing put back, nothing to say.
                    if !names.is_empty() {
                        show(&app, &r.name, &format!("{}. Restored {}.", r.trigger.describe_end(), names.join(", ")));
                    }
                } else {
                    let what =
                        if names.is_empty() { String::new() } else { format!(" Switched to {}.", names.join(", ")) };
                    show(&app, &r.name, &format!("{}.{what}", r.trigger.describe()));
                }
            }
        }
    });
    sets_power_mode
}

fn routine<'a>(routines: &'a [Routine], id: &str) -> Option<&'a Routine> {
    routines.iter().find(|r| r.id == id)
}

fn order(change: Change) -> u8 {
    match change {
        Change::Thermal(_) => 0,
        Change::Battery(_) => 1,
        Change::PowerMode(_) => 2,
    }
}

/// Makes one change; the name of what it switched to, or `None` when nothing changed.
fn apply(app: &AppHandle, change: Change, from: Option<&Routine>) -> Option<&'static str> {
    let core = app.state::<Core>();
    let current = core.state();
    let (target, result) = match change {
        Change::Thermal(mode) => {
            if current.thermal == Some(mode) {
                return None;
            }
            (thermal_name(mode), commands::apply_thermal_mode(app, &core, mode).map(drop))
        }
        Change::Battery(profile) => {
            if current.active_profile == Some(ActiveProfile::Known { profile }) {
                return None;
            }
            (profile_name(profile), commands::apply_battery_profile(app, &core, profile).map(drop))
        }
        Change::PowerMode(mode) => {
            if platform::power_mode::get() == Some(mode) {
                return None;
            }
            let result = platform::power_mode::set(mode).map_err(|e| e.to_string());
            let state = core.update(|s| s.power_mode = platform::power_mode::get());
            let _ = app.emit("state-changed", &state);
            (power_mode_name(mode), result)
        }
    };
    match result {
        Ok(()) => Some(target),
        Err(e) => {
            let name = from.map_or("A routine", |r| r.name.as_str());
            eprintln!("automation: {name}: switching to {target} failed: {e}");
            // A failure is always worth a notification: the user expects the change.
            show(app, &format!("Couldn't switch to {target}"), &format!("{name}: {e}"));
            None
        }
    }
}

fn show(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("automation: notification failed: {e}");
    }
}

fn thermal_name(mode: ThermalMode) -> &'static str {
    match mode {
        ThermalMode::Optimized => "Optimized",
        ThermalMode::Cool => "Cool",
        ThermalMode::Quiet => "Quiet",
        ThermalMode::UltraPerformance => "Ultra Performance",
    }
}

fn profile_name(profile: BatteryProfile) -> &'static str {
    match profile {
        BatteryProfile::Home => "Desk",
        BatteryProfile::Campus => "Remote",
        BatteryProfile::Storage => "Storage",
    }
}

fn power_mode_name(mode: PowerMode) -> &'static str {
    match mode {
        PowerMode::BestEfficiency => "best power efficiency",
        PowerMode::Balanced => "balanced",
        PowerMode::BestPerformance => "best performance",
    }
}
