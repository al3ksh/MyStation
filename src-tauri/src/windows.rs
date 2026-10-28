//! Flyout and full-window lifecycle.
//!
//! Windows are built hidden and shown by `reveal` once their page has painted its first state, so they never
//! flash blank. A dismissed flyout is only hidden, so the next tray click shows it at once; after `FLYOUT_KEEP`
//! unused it is destroyed, and the tray-only app holds no WebView again.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Local;
use mystation_core::tray_icon::{BlurDebounce, ToggleGuard};
use tauri::window::{Effect, EffectsBuilder};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::state::{Core, PollMode};
use crate::sync::LockExt;

pub const FLYOUT: &str = "flyout";
pub const MAIN: &str = "main";
pub const MONITOR: &str = "monitor";

const FLYOUT_W: f64 = 360.0;
/// Until the page reports its content height.
const FLYOUT_H: f64 = 520.0;
const FLYOUT_MIN_H: f64 = 240.0;
const FLYOUT_MAX_H: f64 = 760.0;
const FLYOUT_MARGIN: f64 = 12.0;
/// How long a hidden flyout stays warm for an instant reopen.
const FLYOUT_KEEP: Duration = Duration::from_secs(10 * 60);
/// A page that never reports ready is shown anyway after this long.
const REVEAL_FALLBACK: Duration = Duration::from_millis(1500);

#[derive(Default)]
pub struct WindowsState {
    guard: Mutex<ToggleGuard>,
    /// Physical centre of the tray icon at the last click: picks the monitor for the flyout.
    tray_point: Mutex<Option<(f64, f64)>>,
    /// Bumped on every flyout show: a release timer only fires if no show happened since it was armed.
    flyout_shows: AtomicU64,
    /// Logical flyout height as `f64` bits; 0 = not reported yet.
    flyout_h: AtomicU64,
}

/// The flyout height for a page's content height (logical px).
fn flyout_height(content: f64) -> f64 {
    if content.is_nan() { FLYOUT_H } else { content.clamp(FLYOUT_MIN_H, FLYOUT_MAX_H) }
}

/// Whether a window `actual` physical px tall must be resized to `want` logical px.
fn needs_resize(actual: u32, scale: f64, want: f64) -> bool {
    (f64::from(actual) - want * scale).abs() >= 1.0
}

fn current_flyout_height(app: &AppHandle) -> f64 {
    match app.state::<WindowsState>().flyout_h.load(Ordering::SeqCst) {
        0 => FLYOUT_H,
        bits => f64::from_bits(bits),
    }
}

/// Sizes the flyout to its content; it stays anchored to the bottom-right of the work area.
pub fn fit_flyout(w: &WebviewWindow, content: f64) {
    if w.label() != FLYOUT {
        return;
    }
    let app = w.app_handle();
    let h = flyout_height(content);
    let stored = app.state::<WindowsState>().flyout_h.swap(h.to_bits(), Ordering::SeqCst);
    // Compared with the real size too: Windows can resize the window behind our back (a DPI change at boot).
    let drifted = match (w.inner_size(), w.scale_factor()) {
        (Ok(size), Ok(scale)) => needs_resize(size.height, scale, h),
        _ => true,
    };
    if stored == h.to_bits() && !drifted {
        return;
    }
    let _ = w.set_size(LogicalSize::new(FLYOUT_W, h));
    place_flyout(app, w);
}

pub fn set_tray_point(app: &AppHandle, x: f64, y: f64) {
    *app.state::<WindowsState>().tray_point.lock_ok() = Some((x, y));
}

/// Mouse down on the tray icon: the moment an open flyout loses focus.
pub fn tray_pressed(app: &AppHandle) {
    app.state::<WindowsState>().guard.lock_ok().on_pressed(Local::now().timestamp_millis());
}

pub fn toggle_flyout(app: &AppHandle) {
    let existing = app.get_webview_window(FLYOUT);
    if let Some(w) = &existing
        && w.is_visible().unwrap_or(false)
    {
        hide_flyout(app, w);
        return;
    }
    // A click that arrives right after the flyout hid on blur is the same click that blurred it.
    if !app.state::<WindowsState>().guard.lock_ok().should_open(Local::now().timestamp_millis()) {
        return;
    }
    match existing {
        Some(w) => show_flyout(app, &w),
        None => build_flyout(app),
    }
}

/// Builds the flyout hidden; its page calls `window_ready` once painted, which shows it.
fn build_flyout(app: &AppHandle) {
    let built = WebviewWindowBuilder::new(app, FLYOUT, WebviewUrl::App("index.html".into()))
        .title(crate::commands::display_name())
        .inner_size(FLYOUT_W, current_flyout_height(app))
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(true)
        .effects(EffectsBuilder::new().effect(Effect::Acrylic).build())
        .visible(false)
        .build();
    let Ok(w) = built else { return };
    watch(app, &w);
    reveal_fallback(&w);
    let handle = app.clone();
    let blur = Arc::new(Mutex::new(BlurDebounce::default()));
    w.on_window_event(move |e| {
        let WindowEvent::Focused(focused) = *e else { return };
        let blurred_at = Local::now().timestamp_millis();
        blur.lock_ok().on_focus(focused, blurred_at);
        if focused {
            return;
        }
        let (handle, blur) = (handle.clone(), blur.clone());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(BlurDebounce::SETTLE_MS as u64));
            if !blur.lock_ok().should_hide(Local::now().timestamp_millis()) {
                return;
            }
            let Some(w) = handle.get_webview_window(FLYOUT) else { return };
            if !w.is_visible().unwrap_or(false) {
                return;
            }
            handle.state::<WindowsState>().guard.lock_ok().on_hidden(blurred_at);
            hide_flyout(&handle, &w);
        });
    });
}

fn show_flyout(app: &AppHandle, w: &WebviewWindow) {
    app.state::<WindowsState>().flyout_shows.fetch_add(1, Ordering::SeqCst);
    place_flyout(app, w);
    let _ = w.show();
    let _ = w.set_focus();
    update_poll_mode(app, None);
}

/// Hides the flyout and arms its release: destroyed after `FLYOUT_KEEP` unless shown again first.
fn hide_flyout(app: &AppHandle, w: &WebviewWindow) {
    let _ = w.hide();
    update_poll_mode(app, None);
    let armed_at = app.state::<WindowsState>().flyout_shows.load(Ordering::SeqCst);
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FLYOUT_KEEP);
        if handle.state::<WindowsState>().flyout_shows.load(Ordering::SeqCst) != armed_at {
            return;
        }
        if let Some(w) = handle.get_webview_window(FLYOUT)
            && !w.is_visible().unwrap_or(true)
        {
            let _ = w.destroy();
        }
    });
}

/// Called by a window's page once it has painted its first state.
pub fn reveal(w: &WebviewWindow) {
    if w.is_visible().unwrap_or(false) {
        return;
    }
    let app = w.app_handle();
    if w.label() == FLYOUT {
        show_flyout(app, w);
    } else {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Shows the window even if its page never reports ready, rather than leaving the click without effect.
fn reveal_fallback(w: &WebviewWindow) {
    let w = w.clone();
    std::thread::spawn(move || {
        std::thread::sleep(REVEAL_FALLBACK);
        reveal(&w);
    });
}

/// Bottom-right of the work area of the monitor that holds the tray icon.
fn place_flyout(app: &AppHandle, w: &WebviewWindow) {
    let point = *app.state::<WindowsState>().tray_point.lock_ok();
    let monitor = match point {
        Some((x, y)) => app.monitor_from_point(x, y).ok().flatten(),
        None => None,
    }
    .or_else(|| app.primary_monitor().ok().flatten());
    let Some(m) = monitor else { return };
    let scale = m.scale_factor();
    let area = m.work_area();
    let x = f64::from(area.position.x) + f64::from(area.size.width) - (FLYOUT_W + FLYOUT_MARGIN) * scale;
    let h = current_flyout_height(app);
    let y = f64::from(area.position.y) + f64::from(area.size.height) - (h + FLYOUT_MARGIN) * scale;
    let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

pub fn open_full(app: &AppHandle) {
    open_full_at(app, None);
}

/// A new window starts on `section`; an open one is told to switch to it.
pub fn open_full_at(app: &AppHandle, section: Option<&str>) {
    if let Some(f) = app.get_webview_window(FLYOUT)
        && f.is_visible().unwrap_or(false)
    {
        hide_flyout(app, &f);
    }
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(s) = section {
            let _ = app.emit_to(MAIN, "open-section", s);
        }
        return;
    }
    let start = section.map(|s| format!("window.__SECTION__ = {};", serde_json::Value::from(s))).unwrap_or_default();
    let built = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::App("index.html".into()))
        .initialization_script(&start)
        .title(crate::commands::display_name())
        .inner_size(960.0, 640.0)
        .min_inner_size(760.0, 520.0)
        .effects(EffectsBuilder::new().effect(Effect::Mica).build())
        .center()
        .visible(false)
        .build();
    if let Ok(w) = built {
        watch(app, &w);
        reveal_fallback(&w);
    }
}

/// The sensor monitor: a regular window of its own, so it can stay open beside games and benchmarks.
pub fn open_monitor(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MONITOR) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, MONITOR, WebviewUrl::App("index.html".into()))
        .title("Sensor monitor")
        .inner_size(560.0, 720.0)
        .min_inner_size(440.0, 320.0)
        .effects(EffectsBuilder::new().effect(Effect::Mica).build())
        .center()
        .visible(false)
        .build();
    if let Ok(w) = built {
        watch(app, &w);
        reveal_fallback(&w);
    }
}

/// Live sampling while the full window exists or the flyout is on screen; tray-only polling otherwise.
fn update_poll_mode(app: &AppHandle, closing: Option<&str>) {
    let active = app
        .webview_windows()
        .iter()
        .any(|(label, w)| Some(label.as_str()) != closing && (label != FLYOUT || w.is_visible().unwrap_or(false)));
    app.state::<Core>().set_poll_mode(if active { PollMode::Active } else { PollMode::Idle });
}

fn watch(app: &AppHandle, w: &WebviewWindow) {
    update_poll_mode(app, None);
    let handle = app.clone();
    let label = w.label().to_owned();
    w.on_window_event(move |e| {
        if let WindowEvent::Destroyed = e {
            update_poll_mode(&handle, Some(&label));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flyout_height_follows_content_within_screen_friendly_bounds() {
        assert_eq!(flyout_height(512.0), 512.0);
        assert_eq!(flyout_height(10.0), FLYOUT_MIN_H);
        assert_eq!(flyout_height(5000.0), FLYOUT_MAX_H);
        assert_eq!(flyout_height(f64::NAN), FLYOUT_H);
    }

    #[test]
    fn resizes_when_the_window_drifted_from_the_wanted_height() {
        // 125 %: 523 logical px is 654 physical.
        assert!(!needs_resize(654, 1.25, 523.0));
        assert!(!needs_resize(653, 1.25, 523.0));
        // The window kept an older, taller size.
        assert!(needs_resize(676, 1.25, 523.0));
        assert!(needs_resize(600, 1.0, 523.0));
    }
}
