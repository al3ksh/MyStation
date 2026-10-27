//! Tray icon: monochrome gauge showing the thermal mode, tooltip, native menu.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use mystation_core::dell::{ChargeCfg, ThermalMode};
use mystation_core::profile::{ActiveProfile, BatteryProfile, Profiles};
use mystation_core::tray_icon::{icon_color, needle_fraction, render_gauge_icon};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Listener, Manager, Wry};

use crate::commands;
use crate::platform;
use crate::state::{AppState, Core};
use crate::sync::LockExt;
use crate::windows;

const TRAY_ID: &str = "main";
/// The live poller samples every second; older readings mean no window is open (Idle).
const CPU_MAX_AGE: Duration = Duration::from_secs(5);

const PROFILES: [(BatteryProfile, &str); 3] =
    [(BatteryProfile::Home, "Desk"), (BatteryProfile::Campus, "Remote"), (BatteryProfile::Storage, "Storage")];
const THERMALS: [(ThermalMode, &str); 4] = [
    (ThermalMode::Optimized, "Optimized"),
    (ThermalMode::Cool, "Cool"),
    (ThermalMode::Quiet, "Quiet"),
    (ThermalMode::UltraPerformance, "Ultra Performance"),
];

struct TrayItems {
    profiles: Vec<CheckMenuItem<Wry>>,
    thermals: Vec<CheckMenuItem<Wry>>,
    autostart: CheckMenuItem<Wry>,
}

#[derive(Default)]
struct TrayCache {
    /// Last CPU temperature from the live poller and when it was read.
    cpu: Option<(f32, Instant)>,
    /// Icon colour and thermal mode last pushed to the shell — skip identical updates.
    icon: Option<([u8; 3], Option<ThermalMode>)>,
    tooltip: String,
}

pub struct TrayState {
    items: TrayItems,
    cache: Mutex<TrayCache>,
}

fn charge_label(cfg: ChargeCfg) -> String {
    match cfg {
        ChargeCfg::Custom { start, stop } => format!("{start}–{stop}%"),
        ChargeCfg::Standard => "100%".into(),
        ChargeCfg::Adaptive => "Adaptive".into(),
        ChargeCfg::PrimAcUse => "Primarily AC".into(),
        ChargeCfg::Express => "ExpressCharge".into(),
    }
}

fn profile_item_text(name: &str, profile: BatteryProfile, profiles: &Profiles) -> String {
    format!("{name} {}", charge_label(profiles.get(profile)))
}

fn active_name(active: &ActiveProfile) -> String {
    match active {
        ActiveProfile::Known { profile } => {
            PROFILES.iter().find(|(p, _)| p == profile).map_or("", |(_, n)| n).to_owned()
        }
        ActiveProfile::Other { cfg: ChargeCfg::Custom { start, stop } } => format!("Custom ({start}–{stop}%)"),
        ActiveProfile::Other { cfg } => charge_label(*cfg),
    }
}

/// The CPU temperature only while it is being polled; Idle does not measure it.
fn fresh_cpu(cached: Option<(f32, Instant)>, now: Instant) -> Option<f32> {
    cached.filter(|(_, at)| now.duration_since(*at) <= CPU_MAX_AGE).map(|(c, _)| c)
}

/// `MyPrecision — 82% · Desk · CPU 54 °C`, missing parts skipped.
fn tooltip(name: &str, state: &AppState, cpu_c: Option<f32>) -> String {
    let parts: Vec<String> = [
        state.battery.as_ref().and_then(|b| b.percent).map(|p| format!("{}%", p.round())),
        state.active_profile.as_ref().map(active_name),
        cpu_c.map(|c| format!("CPU {} °C", c.round())),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() { name.into() } else { format!("{name} — {}", parts.join(" · ")) }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let state = app.state::<Core>().state();
    let profiles = PROFILES
        .iter()
        .map(|(p, name)| {
            let id = format!("profile:{name}");
            CheckMenuItem::with_id(app, id, profile_item_text(name, *p, &state.profiles), true, false, None::<&str>)
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let thermals = THERMALS
        .iter()
        .map(|(_, name)| CheckMenuItem::with_id(app, format!("thermal:{name}"), *name, true, false, None::<&str>))
        .collect::<tauri::Result<Vec<_>>>()?;
    let thermal_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = thermals.iter().map(|i| i as _).collect();
    let thermal_menu = Submenu::with_items(app, "Thermal mode", true, &thermal_refs)?;
    let open = MenuItem::with_id(app, "open", format!("Open {}", crate::commands::display_name()), true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "Start at sign-in", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);

    let menu = Menu::new(app)?;
    for item in &profiles {
        menu.append(item)?;
    }
    menu.append(&sep()?)?;
    menu.append(&thermal_menu)?;
    menu.append(&sep()?)?;
    menu.append(&open)?;
    menu.append(&autostart)?;
    menu.append(&sep()?)?;
    menu.append(&quit)?;

    // The icon goes in at creation: when the app starts before Explorer at boot, the shell
    // re-adds it on TaskbarCreated with whatever icon it was built with.
    let (key, icon) = gauge(state.thermal);
    app.manage(TrayState {
        items: TrayItems { profiles, thermals, autostart },
        cache: Mutex::new(TrayCache { icon: Some(key), ..TrayCache::default() }),
    });

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip(crate::commands::display_name())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state, position, .. } = event {
                let app = tray.app_handle();
                match button_state {
                    MouseButtonState::Down => windows::tray_pressed(app),
                    MouseButtonState::Up => {
                        windows::set_tray_point(app, position.x, position.y);
                        windows::toggle_flyout(app);
                    }
                }
            }
        })
        .build(app)?;

    refresh(app, &state, None);
    let handle = app.clone();
    app.listen_any("state-changed", move |_| {
        let state = handle.state::<Core>().state();
        refresh(&handle, &state, None);
    });
    Ok(())
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref().to_owned();
    match id.as_str() {
        "open" => windows::open_full(app),
        "quit" => app.exit(0),
        "autostart" => {
            let handle = app.clone();
            std::thread::spawn(move || {
                let core = handle.state::<Core>();
                // Success and failure both emit state-changed, which re-syncs the check mark.
                let _ = commands::apply_autostart(&handle, &core, !core.state().autostart);
            });
        }
        _ => {
            // cctk takes a few hundred ms per call: keep the menu thread free.
            let handle = app.clone();
            std::thread::spawn(move || {
                let core = handle.state::<Core>();
                let result = if let Some(name) = id.strip_prefix("profile:") {
                    PROFILES
                        .iter()
                        .find(|(_, n)| *n == name)
                        .map(|(p, _)| commands::apply_battery_profile(&handle, &core, *p))
                } else if let Some(name) = id.strip_prefix("thermal:") {
                    THERMALS
                        .iter()
                        .find(|(_, n)| *n == name)
                        .map(|(m, _)| commands::apply_thermal_mode(&handle, &core, *m))
                } else {
                    None
                };
                // Success already emitted state-changed; a failure still has to undo the menu's own toggle.
                if let Some(Err(_)) = result {
                    let state = core.state();
                    refresh(&handle, &state, None);
                    let _ = handle.emit("state-changed", &state);
                }
            });
        }
    }
}

/// The gauge for `thermal` in the taskbar's colour, with the cache key it was drawn for.
fn gauge(thermal: Option<ThermalMode>) -> (([u8; 3], Option<ThermalMode>), Image<'static>) {
    let color = icon_color(platform::light_taskbar());
    let size = platform::tray_icon_size();
    let rgba = render_gauge_icon(color, needle_fraction(thermal), size);
    ((color, thermal), Image::new_owned(rgba, size, size))
}

/// Icon, tooltip and menu checks from `state`. `cpu_c` updates the remembered CPU temperature.
pub fn refresh(app: &AppHandle, state: &AppState, cpu_c: Option<f32>) {
    let Some(tray_state) = app.try_state::<TrayState>() else { return };
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let mut cache = tray_state.cache.lock_ok();
    let now = Instant::now();
    if let Some(c) = cpu_c {
        cache.cpu = Some((c, now));
    }

    let color = icon_color(platform::light_taskbar());
    if cache.icon != Some((color, state.thermal)) {
        let (key, icon) = gauge(state.thermal);
        if tray.set_icon(Some(icon)).is_ok() {
            cache.icon = Some(key);
        }
    }
    let text = tooltip(crate::commands::display_name(), state, fresh_cpu(cache.cpu, now));
    if cache.tooltip != text {
        let _ = tray.set_tooltip(Some(&text));
        cache.tooltip = text;
    }
    drop(cache);

    let items = &tray_state.items;
    for ((p, name), item) in PROFILES.iter().zip(&items.profiles) {
        let active = state.active_profile == Some(ActiveProfile::Known { profile: *p });
        let _ = item.set_checked(active);
        let _ = item.set_text(profile_item_text(name, *p, &state.profiles));
    }
    for ((m, _), item) in THERMALS.iter().zip(&items.thermals) {
        let _ = item.set_checked(state.thermal == Some(*m));
    }
    let _ = items.autostart.set_checked(state.autostart);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Availability;
    use mystation_core::battery::{BatterySnapshot, PowerState};

    fn state(percent: Option<f32>, active: Option<ActiveProfile>) -> AppState {
        AppState {
            charge: None,
            active_profile: active,
            thermal: None,
            profiles: Profiles::default(),
            battery: percent.map(|p| BatterySnapshot {
                percent: Some(p),
                state: PowerState::Discharging,
                power_w: -10.0,
                remaining_mwh: 0,
                full_mwh: None,
                design_mwh: None,
                wear_pct: None,
                cycles: None,
                voltage_v: 0.0,
            }),
            availability: Availability { cctk: true, wmi: true, dcm: true, admin: true, optimizer_running: false },
            autostart: false,
            optimizer_warning_dismissed: false,
            peripherals: Vec::new(),
            power_mode: None,
            energy_saver: false,
            match_power_mode: false,
        }
    }

    #[test]
    fn tooltip_full() {
        let s = state(Some(82.4), Some(ActiveProfile::Known { profile: BatteryProfile::Home }));
        assert_eq!(tooltip("MyXPS", &s, Some(54.2)), "MyXPS — 82% · Desk · CPU 54 °C");
    }

    #[test]
    fn tooltip_skips_missing_parts() {
        assert_eq!(tooltip("MyStation", &state(None, None), None), "MyStation");
        let other = ActiveProfile::Other { cfg: ChargeCfg::Custom { start: 60, stop: 90 } };
        assert_eq!(tooltip("MyStation", &state(Some(50.0), Some(other)), None), "MyStation — 50% · Custom (60–90%)");
    }

    #[test]
    fn cpu_reading_expires_once_live_polling_stops() {
        let t0 = Instant::now();
        assert_eq!(fresh_cpu(Some((54.0, t0)), t0 + Duration::from_secs(2)), Some(54.0));
        assert_eq!(fresh_cpu(Some((54.0, t0)), t0 + Duration::from_secs(30)), None);
        assert_eq!(fresh_cpu(None, t0), None);
    }

    #[test]
    fn profile_items_follow_config() {
        let p = Profiles::default();
        assert_eq!(profile_item_text("Desk", BatteryProfile::Home, &p), "Desk 75–80%");
        assert_eq!(profile_item_text("Remote", BatteryProfile::Campus, &p), "Remote 100%");
        assert_eq!(profile_item_text("Storage", BatteryProfile::Storage, &p), "Storage 50–60%");
    }
}
