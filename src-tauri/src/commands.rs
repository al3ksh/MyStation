//! Tauri commands. Errors reach the UI as English sentences.

use chrono::Local;
use mystation_core::automation::{self, Automation};
use mystation_core::bios;
use mystation_core::boot;
use mystation_core::config;
use mystation_core::dell::{DellError, ThermalMode};
use mystation_core::energy;
use mystation_core::gpu;
use mystation_core::history::{HealthEntry, HistorySample};
use mystation_core::nvme;
use mystation_core::profile::BatteryProfile;
use mystation_core::sleep;
use mystation_core::usb;
use mystation_core::display;
use tauri::{AppHandle, Emitter, State};

use crate::platform;
use crate::state::{AppState, Core, config_path, data_dir};
use crate::sync::LockExt;

/// After a write attempt the UI must show the truth: a success carries the value the BIOS
/// accepted; a failure leaves both settings uncertain, so re-read them.
fn after_write<T>(
    app: &AppHandle,
    core: &Core,
    result: Result<T, DellError>,
    store: impl FnOnce(&Core, T) -> AppState,
) -> Result<AppState, String> {
    let outcome = match result {
        Ok(read_back) => Ok(store(core, read_back)),
        Err(e) => {
            core.refresh_bios();
            Err(e.to_string())
        }
    };
    let _ = app.emit("state-changed", &core.state());
    outcome
}

#[tauri::command]
pub fn get_state(core: State<'_, Core>) -> AppState {
    core.state()
}

/// Shared by the command and the tray menu.
pub fn apply_battery_profile(app: &AppHandle, core: &Core, profile: BatteryProfile) -> Result<AppState, String> {
    let Some(cctk) = &core.cctk else {
        return Err(DellError::NotInstalled.to_string());
    };
    let cfg = core.config.lock_ok().profiles.get(profile);
    let result = cctk.set_charge_cfg(cfg);
    after_write(app, core, result, Core::set_charge)
}

/// Shared by the command and the tray menu.
pub fn apply_thermal_mode(app: &AppHandle, core: &Core, mode: ThermalMode) -> Result<AppState, String> {
    let Some(cctk) = &core.cctk else {
        return Err(DellError::NotInstalled.to_string());
    };
    let result = cctk.set_thermal(mode);
    after_write(app, core, result, Core::set_thermal)
}

#[tauri::command(async)]
pub fn set_battery_profile(app: AppHandle, core: State<'_, Core>, profile: BatteryProfile) -> Result<AppState, String> {
    apply_battery_profile(&app, &core, profile)
}

#[tauri::command(async)]
pub fn set_thermal_mode(app: AppHandle, core: State<'_, Core>, mode: ThermalMode) -> Result<AppState, String> {
    apply_thermal_mode(&app, &core, mode)
}

#[tauri::command]
pub fn get_history(core: State<'_, Core>, minutes: u32) -> Vec<HistorySample> {
    core.history.lock_ok().range(minutes, Local::now().timestamp_millis())
}

/// The latest sample, so a freshly opened window doesn't wait for the next tick.
#[tauri::command]
pub fn get_telemetry(core: State<'_, Core>) -> Option<mystation_core::sensors::Telemetry> {
    core.telemetry.lock_ok().clone()
}

#[tauri::command]
pub fn get_health_log(core: State<'_, Core>) -> Vec<HealthEntry> {
    core.health.lock_ok().entries().to_vec()
}

/// Shared by the command and the tray menu. Returns the real state after the attempt.
pub fn apply_autostart(app: &AppHandle, core: &Core, enabled: bool) -> Result<bool, String> {
    let result = if enabled { crate::autostart::enable() } else { crate::autostart::disable() };
    let actual = crate::autostart::is_enabled();
    let state = core.update(|s| s.autostart = actual);
    let _ = app.emit("state-changed", &state);
    result.map(|()| actual)
}

#[tauri::command(async)]
pub fn set_autostart(app: AppHandle, core: State<'_, Core>, enabled: bool) -> Result<bool, String> {
    apply_autostart(&app, &core, enabled)
}

#[tauri::command(async)]
pub fn dismiss_optimizer_warning(app: AppHandle, core: State<'_, Core>) {
    {
        let mut cfg = core.config.lock_ok();
        cfg.optimizer_warning_dismissed = true;
        // Best effort: the flag still holds for this session if the disk write fails.
        let _ = config::save(&config_path(), &cfg);
    }
    let state = core.update(|s| s.optimizer_warning_dismissed = true);
    let _ = app.emit("state-changed", &state);
}

#[tauri::command(async)]
pub fn open_full_window(app: AppHandle) {
    crate::windows::open_full(&app);
}

/// The flyout's content height (CSS px): the window is sized to it.
#[tauri::command(async)]
pub fn fit_flyout(window: tauri::WebviewWindow, height: f64) {
    crate::windows::fit_flyout(&window, height);
}

/// The calling window has painted its first state: show it now, never blank.
#[tauri::command(async)]
pub fn window_ready(window: tauri::WebviewWindow) {
    crate::windows::reveal(&window);
}

/// Model, service tag and BIOS version from SMBIOS; read once, it never changes while running.
pub fn device_info() -> &'static mystation_core::smbios::DeviceInfo {
    static INFO: std::sync::OnceLock<mystation_core::smbios::DeviceInfo> = std::sync::OnceLock::new();
    INFO.get_or_init(crate::platform::device_info)
}

/// The name shown in the UI: "MyXPS" on an XPS, "MyStation" outside the known Dell lines.
pub fn display_name() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| mystation_core::brand::app_name(device_info().model.as_deref()))
}

#[tauri::command(async)]
pub fn get_device_info() -> mystation_core::smbios::DeviceInfo {
    device_info().clone()
}

#[tauri::command(async)]
pub fn get_app_name() -> &'static str {
    display_name()
}

#[tauri::command]
pub fn get_automation(core: State<'_, Core>) -> Automation {
    core.config.lock_ok().automation.clone()
}

#[tauri::command(async)]
pub fn set_automation(core: State<'_, Core>, rules: Automation) -> Result<Automation, String> {
    automation::validate(&rules)?;
    {
        let mut cfg = core.config.lock_ok();
        let mut next = cfg.clone();
        next.automation = rules.clone();
        config::save(&config_path(), &next).map_err(|e| format!("Could not save settings: {e}"))?;
        *cfg = next;
    }
    let mut state = core.automation.lock_ok();
    state.rules_changed(Local::now().naive_local());
    crate::automation::persist(&state);
    Ok(rules)
}

/// The curated BIOS settings with their current values; one cctk run (seconds).
#[tauri::command(async)]
pub fn get_bios_settings(core: State<'_, Core>) -> Result<Vec<bios::Setting>, String> {
    let Some(cctk) = &core.cctk else {
        return Err(DellError::NotInstalled.to_string());
    };
    bios::read(cctk).map_err(|e| e.to_string())
}

/// Writes one whitelisted setting; the UI confirms with the user before calling this.
#[tauri::command(async)]
pub fn set_bios_setting(core: State<'_, Core>, key: String, value: String) -> Result<(), String> {
    let Some(cctk) = &core.cctk else {
        return Err(DellError::NotInstalled.to_string());
    };
    bios::write(cctk, &key, &value).map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveReport {
    model: Option<String>,
    nvme: bool,
    smart: Option<nvme::SmartLog>,
    /// Years to rated endurance at the write rate since `tracking_since`.
    years_left: Option<f64>,
    tracking_since: Option<chrono::NaiveDate>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageReport {
    drives: Vec<DriveReport>,
    /// Total and free bytes of the Windows volume.
    volume: Option<(u64, u64)>,
}

/// Drive health; the first reading of each drive is kept to forecast its wear.
#[tauri::command(async)]
pub fn get_storage() -> StorageReport {
    let today = Local::now().date_naive();
    let drives = platform::storage::drives()
        .into_iter()
        .map(|d| {
            let first = d.smart.as_ref().zip(d.identity.serial.as_deref()).map(|(s, serial)| {
                let now =
                    nvme::WearReading { date: today, bytes_written: s.bytes_written, percent_used: s.percent_used };
                (nvme::first_reading(&data_dir().join("ssd.json"), serial, now), now)
            });
            DriveReport {
                model: d.identity.model,
                nvme: d.identity.nvme,
                smart: d.smart,
                years_left: first.and_then(|(first, now)| nvme::years_left(&first, &now)),
                tracking_since: first.map(|(first, _)| first.date),
            }
        })
        .collect();
    StorageReport { drives, volume: platform::storage::system_volume() }
}

const BOOT_CHANNEL: &str = "Microsoft-Windows-Diagnostics-Performance/Operational";
const SESSION_POWER: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Power";
/// Boots shown and culprits counted over.
const BOOTS_KEPT: usize = 10;

/// Recent boot times and what slowed them; the log needs administrator rights.
#[tauri::command(async)]
pub fn get_boot() -> Result<boot::BootReport, String> {
    let ids = boot::EVENT_IDS.map(|id| format!("EventID={id}")).join(" or ");
    let events = platform::event_log::query(BOOT_CHANNEL, &format!("*[System[({ids})]]"), 500)
        .map_err(|_| "Startup history needs administrator rights.".to_string())?;
    Ok(boot::BootReport {
        firmware_ms: platform::registry::hklm_dword(SESSION_POWER, "FwPOSTTime").filter(|&ms| ms > 0).map(u64::from),
        fast_startup: platform::registry::hklm_dword(SESSION_POWER, "HiberbootEnabled") == Some(1),
        ..boot::report(&events, BOOTS_KEPT)
    })
}

const BATTERY_ADMIN: &str = "Battery history needs administrator rights.";

/// Battery energy per app over the last day and week, as Windows estimates it.
#[tauri::command(async)]
pub fn get_app_energy() -> Result<energy::EnergyReport, String> {
    if !platform::is_elevated() {
        return Err(BATTERY_ADMIN.into());
    }
    let csv = platform::powercfg::srum_csv().map_err(|_| "Couldn't read battery use by app.".to_string())?;
    Ok(energy::report(&csv, chrono::Utc::now()))
}

/// Sleep sessions on battery over the last week.
#[tauri::command(async)]
pub fn get_sleep() -> Result<sleep::SleepReport, String> {
    if !platform::is_elevated() {
        return Err(BATTERY_ADMIN.into());
    }
    let xml = platform::powercfg::sleep_study_xml().map_err(|_| "Couldn't read the sleep history.".to_string())?;
    Ok(sleep::report(&xml))
}

/// Plugged-in USB devices and the battery draw each added when it arrived.
#[tauri::command(async)]
pub fn get_usb(core: State<'_, Core>) -> usb::UsbReport {
    let devices = platform::usb::devices();
    let samples = core.history.lock_ok().range(30, Local::now().timestamp_millis());
    usb::report(devices, &samples, &mut core.usb_draw.lock_ok())
}

/// Connected displays, and whether each could run sharper or faster than it does.
#[tauri::command(async)]
pub fn get_displays() -> Vec<display::Display> {
    display::report(platform::display::displays())
}

/// What the laptop has and supports, read from the device itself.
#[tauri::command(async)]
pub fn get_spec() -> mystation_core::spec::Spec {
    platform::spec::spec(device_info())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuApp {
    name: String,
    path: String,
    /// Memory held on the discrete GPU; 0 for apps listed only by their rule.
    bytes: u64,
    /// The GPU it's set to; takes effect the next time the app starts.
    choice: gpu::GpuChoice,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DgpuReport {
    name: Option<String>,
    /// Powered on; when off, no app holds it.
    active: bool,
    /// Apps keeping it awake, by memory held.
    apps: Vec<GpuApp>,
    /// Apps with a GPU set, by name.
    rules: Vec<GpuApp>,
}

fn app_name(path: &str) -> String {
    std::path::Path::new(path).file_stem().map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

/// Windows components and NVIDIA's own services hold the card too, but aren't the user's to move.
fn movable(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let own = std::env::current_exe().ok().is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(path));
    !own && !lower.starts_with(r"c:\windows\") && !lower.contains(r"\nvidia corporation\")
}

/// Who keeps the NVIDIA GPU awake; `None` on machines with integrated graphics only.
#[tauri::command(async)]
pub fn get_dgpu() -> Option<DgpuReport> {
    let adapter = platform::nvidia_adapter()?;
    let active = adapter.power == platform::DevicePower::D0;
    let prefs = platform::gpu_apps::preferences();
    let choice_of = |path: &str| {
        prefs
            .iter()
            .find(|(p, _)| p.eq_ignore_ascii_case(path))
            .map_or(gpu::GpuChoice::Auto, |(_, d)| gpu::GpuChoice::from_data(d))
    };

    let mut apps: Vec<GpuApp> = Vec::new();
    // Counters are read only while the card is on; a sleeping card has no holders.
    for (pid, bytes) in adapter.luid.filter(|_| active).map(platform::gpu_apps::holders).unwrap_or_default() {
        let Some(path) = platform::gpu_apps::process_path(pid).filter(|p| movable(p)) else { continue };
        match apps.iter_mut().find(|a| a.path.eq_ignore_ascii_case(&path)) {
            Some(app) => app.bytes += bytes,
            None => apps.push(GpuApp { name: app_name(&path), choice: choice_of(&path), path, bytes }),
        }
    }
    apps.sort_by_key(|a| std::cmp::Reverse(a.bytes));

    let mut rules: Vec<GpuApp> = prefs
        .iter()
        .map(|(p, d)| GpuApp { name: app_name(p), path: p.clone(), bytes: 0, choice: gpu::GpuChoice::from_data(d) })
        .filter(|a| a.choice != gpu::GpuChoice::Auto)
        .collect();
    rules.sort_by_key(|a| a.name.to_lowercase());
    Some(DgpuReport { name: adapter.name, active, apps, rules })
}

/// Runs `path` on `choice` from its next start; `Auto` returns it to the Windows default.
#[tauri::command(async)]
pub fn set_gpu_choice(path: String, choice: gpu::GpuChoice) -> Result<(), String> {
    if !std::path::Path::new(&path).is_absolute() || !path.to_ascii_lowercase().ends_with(".exe") {
        return Err("That isn't an app's full path.".into());
    }
    let current = platform::gpu_apps::preference(&path);
    let data = gpu::with_preference(current.as_deref(), choice.preference());
    platform::gpu_apps::set_preference(&path, data.as_deref())
        .map_err(|_| "Windows didn't accept the graphics setting.".into())
}

/// Lets the user pick an app to set a GPU for; `None` when they cancel.
#[tauri::command(async)]
pub fn pick_app(window: tauri::WebviewWindow) -> Option<String> {
    platform::file_dialog::pick_exe(window.hwnd().ok().map(|h| h.0 as isize))
}
