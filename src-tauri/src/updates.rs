//! Background check for a new release, so both windows can mention it without the user asking.
//!
//! Only checks: downloading and installing stay a click in Settings.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::state::Core;

/// After start-up settles, so the check never competes with the first BIOS read.
const FIRST_CHECK: Duration = Duration::from_secs(30);
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            tauri::async_runtime::block_on(check(&app));
            std::thread::sleep(EVERY);
        }
    });
}

/// A failed check (offline, GitHub down) keeps what the last one found.
async fn check(app: &AppHandle) {
    let Ok(updater) = app.updater() else { return };
    let Ok(found) = updater.check().await else { return };
    let version = found.map(|u| u.version);
    let core = app.state::<Core>();
    if core.state().update_version == version {
        return;
    }
    let state = core.update(|s| s.update_version = version);
    let _ = app.emit("state-changed", &state);
}
