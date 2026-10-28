//! Sensor thread. `Active` (a window is open): every sensor each second, streamed as
//! `telemetry`. `Idle` (tray only): battery every 30 s. BIOS modes are read once at startup on a
//! helper thread; after that only the app's own writes change them, and each write reads back.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use chrono::Local;
use mystation_core::automation::Inputs;
use mystation_core::bluetooth::{self, BtDevice, LowBattery};
use mystation_core::display::Connection;
use mystation_core::history::{HealthEntry, HistorySample};
use mystation_core::power_mode::SourceWatch;
use mystation_core::sensors::{
    BatteryRaw, CpuSnapshot, CpuTimes, PowerWindow, Telemetry, battery_from_raw, load_between, parse_dcim,
    power_flow,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::platform::{self, CpuPerfReader, CpuPowerReader, GpuReader, WmiReaders};
use crate::state::{Core, PollMode};
use crate::sync::LockExt;

const ACTIVE_INTERVAL: Duration = Duration::from_secs(1);
const IDLE_INTERVAL: Duration = Duration::from_secs(30);
/// CPU load is a delta between two samples; after Idle the previous one is stale.
const LOAD_BASELINE: Duration = Duration::from_millis(250);

pub fn spawn(app: AppHandle, rx: Receiver<PollMode>) {
    std::thread::Builder::new()
        .name("poller".into())
        .spawn(move || Poller::new(app).run(rx))
        .expect("failed to spawn poller thread");
}

struct Poller {
    app: AppHandle,
    // WMI connections are !Send, so they are created on this thread.
    wmi: Option<WmiReaders>,
    gpu: GpuReader,
    cpu_power: Option<CpuPowerReader>,
    cpu_perf: Option<CpuPerfReader>,
    power_window: PowerWindow,
    /// Refreshed on the slow tick: adapters get swapped.
    adapter_w: Option<u32>,
    cpu_prev: CpuTimes,
    last_slow: Option<Instant>,
    bios_read: bool,
    bios_busy: Arc<AtomicBool>,
    low_peripherals: LowBattery,
    source: SourceWatch,
    /// Routine inputs read on the slow tick; `None` until the first read.
    wifi: Option<Option<String>>,
    displays: Option<u32>,
    bt_connected: Option<Vec<String>>,
}

impl Poller {
    fn new(app: AppHandle) -> Self {
        let wmi = WmiReaders::new().ok();
        let dcm = wmi.as_ref().is_some_and(WmiReaders::dcm_available);
        app.state::<Core>().update(|s| {
            s.availability.wmi = wmi.is_some();
            s.availability.dcm = dcm;
        });
        Self {
            app,
            wmi,
            gpu: GpuReader::new(),
            cpu_power: CpuPowerReader::new(),
            cpu_perf: CpuPerfReader::new(),
            power_window: PowerWindow::default(),
            adapter_w: None,
            cpu_prev: platform::cpu_times(),
            last_slow: None,
            bios_read: false,
            bios_busy: Arc::default(),
            low_peripherals: LowBattery::default(),
            source: SourceWatch::default(),
            wifi: None,
            displays: None,
            bt_connected: None,
        }
    }

    fn run(mut self, rx: Receiver<PollMode>) {
        let mut mode = PollMode::Idle;
        loop {
            self.tick(mode);
            let interval = if mode == PollMode::Active { ACTIVE_INTERVAL } else { IDLE_INTERVAL };
            match rx.recv_timeout(interval) {
                Ok(next) => {
                    if next == PollMode::Idle {
                        self.gpu.release();
                        self.power_window = PowerWindow::default();
                        *self.app.state::<Core>().telemetry.lock_ok() = None;
                    } else if mode == PollMode::Idle {
                        self.cpu_prev = platform::cpu_times();
                        if let Some(r) = &mut self.cpu_power {
                            r.read();
                        }
                        if let Some(r) = &mut self.cpu_perf {
                            r.read();
                        }
                        std::thread::sleep(LOAD_BASELINE);
                    }
                    mode = next;
                    // A mode switch samples the cheap sensors at once.
                    self.last_slow = None;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn tick(&mut self, mode: PollMode) {
        let raw = self.wmi.as_ref().and_then(WmiReaders::battery);
        if let Some(r) = &raw {
            let inputs = Inputs {
                battery_pct: battery_from_raw(r).percent,
                wifi: self.wifi.clone(),
                displays: self.displays,
                bluetooth: self.bt_connected.clone(),
                ..Inputs::at(Local::now().naive_local(), r.power_online)
            };
            let routed = crate::automation::tick(&self.app, inputs);
            self.follow_source(r.power_online, routed);
        }
        if mode == PollMode::Active {
            self.sample_telemetry(raw.as_ref());
        }
        if self.last_slow.is_none_or(|t| t.elapsed() >= IDLE_INTERVAL) {
            self.last_slow = Some(Instant::now());
            self.slow_tick(raw.as_ref());
        }
        if bios_due(self.bios_read) {
            self.bios_read = true;
            self.spawn_bios_refresh();
        }
    }

    fn sample_telemetry(&mut self, raw: Option<&BatteryRaw>) {
        let now = platform::cpu_times();
        let load = load_between(self.cpu_prev, now);
        self.cpu_prev = now;
        let dcim = self.wmi.as_ref().and_then(WmiReaders::dcim).map(|rows| parse_dcim(&rows)).unwrap_or_default();
        let gpu = self.gpu.read();
        let battery = raw.map(battery_from_raw);
        let cpu_w = self.cpu_power.as_mut().and_then(CpuPowerReader::read);
        let (clock_mhz, limit_pct) = self.cpu_perf.as_mut().map_or((None, None), CpuPerfReader::read);
        let gpu_w = self.gpu.power_w(&gpu);
        let power = battery.as_ref().map(|b| {
            let (cpu_w, gpu_w) = self.power_window.align(b, cpu_w, gpu_w);
            power_flow(b, cpu_w, gpu_w, self.adapter_w)
        });
        let telemetry = Telemetry {
            ts_ms: Local::now().timestamp_millis(),
            battery,
            cpu: CpuSnapshot { load_pct: load, temp_c: dcim.cpu_c, clock_mhz, limit_pct },
            gpu,
            fans: dcim.fans,
            mem: platform::mem(),
            dimm_c: dcim.dimm_c,
            skin_c: dcim.skin_c,
            power,
        };
        let core = self.app.state::<Core>();
        core.history.lock_ok().push(HistorySample::from(&telemetry));
        *core.telemetry.lock_ok() = Some(telemetry.clone());
        let state = core.update(|s| s.battery = telemetry.battery.clone());
        crate::tray::refresh(&self.app, &state, telemetry.cpu.temp_c);
        let _ = self.app.emit("telemetry", &telemetry);
    }

    fn slow_tick(&mut self, raw: Option<&BatteryRaw>) {
        let core = self.app.state::<Core>();
        let before = core.state();
        let battery = raw.map(battery_from_raw);
        if let Some(b) = &battery
            && let (Some(full_mwh), Some(design_mwh)) = (b.full_mwh, b.design_mwh)
        {
            let entry = HealthEntry { date: Local::now().date_naive(), full_mwh, design_mwh, cycles: b.cycles };
            let _ = core.health.lock_ok().record(entry);
        }
        self.adapter_w = self.wmi.as_ref().and_then(WmiReaders::adapter_w);
        let optimizer_running = platform::optimizer_running();
        // The user can change these in Windows at any time.
        let power_mode = platform::power_mode::get();
        let energy_saver = platform::power_mode::energy_saver();
        let nodes = platform::bluetooth::nodes();
        self.bt_connected = Some(bluetooth::names(&nodes, true));
        let peripherals = bluetooth::devices(nodes);
        self.wifi = platform::wifi::current_ssid();
        let external = platform::display::displays().iter().filter(|d| d.connection != Connection::Internal).count();
        self.displays = Some(external as u32);
        for d in self.low_peripherals.check(&peripherals) {
            self.warn_low(d);
        }
        let after = core.update(|s| {
            s.battery = battery;
            s.availability.optimizer_running = optimizer_running;
            s.peripherals = peripherals;
            s.power_mode = power_mode;
            s.energy_saver = energy_saver;
        });
        if after.availability != before.availability
            || after.peripherals != before.peripherals
            || after.power_mode != before.power_mode
            || after.energy_saver != before.energy_saver
        {
            let _ = self.app.emit("state-changed", &after);
        }
    }

    /// Windows brings back its own power mode for the new source on plug in or out; with the
    /// modes matched, set the one that goes with the thermal mode again. Unless a routine that
    /// just ran sets the power mode itself.
    fn follow_source(&mut self, on_ac: bool, routed: bool) {
        let core = self.app.state::<Core>();
        let matched = core.config.lock_ok().match_power_mode;
        if self.source.observe(on_ac, matched, core.state().thermal).is_none() || routed {
            return;
        }
        let app = self.app.clone();
        std::thread::spawn(move || {
            // Let Windows finish its own switch first, or it lands on top of ours.
            std::thread::sleep(Duration::from_secs(2));
            crate::commands::sync_power_mode(&app, &app.state::<Core>());
        });
    }

    fn warn_low(&self, d: &BtDevice) {
        let body = format!("{}% left. Charge it soon.", d.battery_pct);
        if let Err(e) = self.app.notification().builder().title(format!("{} is running low", d.name)).body(body).show() {
            eprintln!("poller: notification failed: {e}");
        }
    }

    /// Reads the BIOS modes off the poller thread: the cctk process takes ~7 s, which would
    /// otherwise freeze the 1 s telemetry stream.
    fn spawn_bios_refresh(&self) {
        if self.bios_busy.swap(true, Ordering::AcqRel) {
            return;
        }
        let app = self.app.clone();
        let busy = self.bios_busy.clone();
        std::thread::spawn(move || {
            let core = app.state::<Core>();
            let before = core.state();
            let after = core.refresh_bios();
            crate::tray::refresh(&app, &after, None);
            if after.active_profile != before.active_profile || after.thermal != before.thermal {
                let _ = app.emit("state-changed", &after);
            }
            busy.store(false, Ordering::Release);
        });
    }
}

/// cctk reads go through Dell's SMBIOS calls, which freeze every core for milliseconds at a time
/// over several seconds: the touchpad lags. So the poller reads once and never polls.
fn bios_due(read_before: bool) -> bool {
    !read_before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bios_is_read_once_at_startup() {
        assert!(bios_due(false));
    }

    #[test]
    fn bios_is_never_re_read_by_the_poller() {
        // A cctk read stalls every core for seconds (Dell SMIs), which lags the touchpad.
        assert!(!bios_due(true));
    }
}
