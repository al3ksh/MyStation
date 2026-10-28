pub mod automation;
pub mod autostart;
pub mod commands;
pub mod migrate;
pub mod platform;
pub mod poller;
pub mod probe;
pub mod state;
pub mod sync;
pub mod tray;
pub mod updates;
pub mod windows;

use tauri::{Manager, RunEvent};

use crate::state::Core;

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| windows::open_full(app)))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::set_battery_profile,
            commands::set_thermal_mode,
            commands::set_match_power_mode,
            commands::get_history,
            commands::get_telemetry,
            commands::get_health_log,
            commands::set_autostart,
            commands::dismiss_optimizer_warning,
            commands::open_full_window,
            commands::open_monitor,
            commands::get_records,
            commands::window_ready,
            commands::fit_flyout,
            commands::get_device_info,
            commands::get_app_name,
            commands::get_routines,
            commands::set_routines,
            commands::routine_options,
            commands::get_bios_settings,
            commands::set_bios_setting,
            commands::get_storage,
            commands::get_boot,
            commands::get_dgpu,
            commands::get_app_energy,
            commands::get_sleep,
            commands::get_usb,
            commands::get_displays,
            commands::get_spec,
            commands::set_gpu_choice,
            commands::pick_app,
        ])
        .setup(|app| {
            migrate::run();
            let (core, rx) = Core::new();
            let setup_notice = core.take_setup_notice();
            app.manage(core);
            app.manage(windows::WindowsState::default());
            tray::build(app.handle())?;
            // Starts in Idle: tray only, no WebView.
            poller::spawn(app.handle().clone(), rx);
            updates::spawn(app.handle().clone());
            // Without Dell Command | Configure the app can only watch; its banner says what to install.
            if setup_notice {
                windows::open_full(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, event| {
        // Tray app: stay alive when the last window closes.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
