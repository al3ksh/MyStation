//! Shared app core: BIOS access, config, history and the last state shown to the UI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use mystation_core::automation::{self, AutomationState};
use mystation_core::bluetooth::BtDevice;
use mystation_core::config::{self, Config};
use mystation_core::dell::{Cctk, ChargeCfg, ThermalMode};
use mystation_core::history::{HealthLog, History};
use mystation_core::power_mode::PowerMode;
use mystation_core::profile::{ActiveProfile, Profiles, detect};
use mystation_core::sensors::{BatterySnapshot, Telemetry};
use serde::Serialize;

use crate::platform::{self, ExeCctkRunner};
use crate::sync::LockExt;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub cctk: bool,
    pub wmi: bool,
    pub dcm: bool,
    pub admin: bool,
    pub optimizer_running: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub charge: Option<ChargeCfg>,
    pub active_profile: Option<ActiveProfile>,
    pub thermal: Option<ThermalMode>,
    pub profiles: Profiles,
    pub battery: Option<BatterySnapshot>,
    pub availability: Availability,
    pub autostart: bool,
    pub optimizer_warning_dismissed: bool,
    /// Connected Bluetooth devices that report a battery, emptiest first.
    pub peripherals: Vec<BtDevice>,
    /// The Windows power mode for the current power source.
    pub power_mode: Option<PowerMode>,
    /// Energy Saver is on: Windows runs at best power efficiency whatever the power mode.
    pub energy_saver: bool,
    pub match_power_mode: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollMode {
    Active,
    Idle,
}

pub struct Core {
    pub cctk: Option<Cctk<ExeCctkRunner>>,
    pub config: Mutex<Config>,
    pub history: Mutex<History>,
    pub health: Mutex<HealthLog>,
    pub snapshot: Mutex<AppState>,
    /// Last telemetry sample while a window is open, so a window can paint it before the next tick.
    pub telemetry: Mutex<Option<Telemetry>>,
    pub automation: Mutex<AutomationState>,
    /// Last measured battery draw per USB device ID, for this session.
    pub usb_draw: Mutex<HashMap<String, f32>>,
    poller_tx: Sender<PollMode>,
}

/// `%APPDATA%\MyPrecision`: the pre-rebrand name, kept so existing settings and history carry over.
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    base.join("MyPrecision")
}

pub fn config_path() -> PathBuf {
    data_dir().join("config.json")
}

impl Core {
    /// The receiver goes to the poller thread.
    /// True on the first start without Dell Command | Configure; the flag is saved best effort.
    pub fn take_setup_notice(&self) -> bool {
        let mut cfg = self.config.lock_ok();
        let show = cfg.take_setup_notice(self.cctk.is_none());
        if show {
            let _ = config::save(&config_path(), &cfg);
        }
        show
    }

    pub fn new() -> (Self, Receiver<PollMode>) {
        let (poller_tx, rx) = channel();
        let config = config::load(&config_path());
        let cctk = ExeCctkRunner::locate().map(Cctk::new);
        let snapshot = AppState {
            charge: None,
            active_profile: None,
            thermal: None,
            profiles: config.profiles.clone(),
            battery: None,
            availability: Availability {
                cctk: cctk.is_some(),
                wmi: false,
                dcm: false,
                admin: platform::is_elevated(),
                optimizer_running: false,
            },
            autostart: crate::autostart::is_enabled(),
            optimizer_warning_dismissed: config.optimizer_warning_dismissed,
            peripherals: Vec::new(),
            power_mode: platform::power_mode::get(),
            energy_saver: platform::power_mode::energy_saver(),
            match_power_mode: config.match_power_mode,
        };
        let core = Self {
            cctk,
            config: Mutex::new(config),
            history: Mutex::new(History::new()),
            health: Mutex::new(HealthLog::open(&data_dir().join("battery-health.json"))),
            snapshot: Mutex::new(snapshot),
            telemetry: Mutex::default(),
            automation: Mutex::new(automation::load_state(&crate::automation::state_path())),
            usb_draw: Mutex::default(),
            poller_tx,
        };
        (core, rx)
    }

    pub fn set_poll_mode(&self, m: PollMode) {
        // The poller only stops with the app; a send error then is harmless.
        let _ = self.poller_tx.send(m);
    }

    pub fn state(&self) -> AppState {
        self.snapshot.lock_ok().clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut AppState)) -> AppState {
        let mut s = self.snapshot.lock_ok();
        f(&mut s);
        s.clone()
    }

    /// Store a charge setting read back from the BIOS, re-detecting the active profile.
    pub fn set_charge(&self, charge: ChargeCfg) -> AppState {
        let profiles = self.config.lock_ok().profiles.clone();
        self.update(|s| {
            s.charge = Some(charge);
            s.active_profile = Some(detect(charge, &profiles));
            s.profiles = profiles.clone();
        })
    }

    pub fn set_thermal(&self, mode: ThermalMode) -> AppState {
        self.update(|s| s.thermal = Some(mode))
    }

    /// Re-read the BIOS settings through cctk into the snapshot.
    pub fn refresh_bios(&self) -> AppState {
        let (charge, thermal) = match &self.cctk {
            Some(cctk) => cctk.read_bios(),
            None => (None, None),
        };
        let profiles = self.config.lock_ok().profiles.clone();
        self.update(|s| {
            s.charge = charge;
            s.active_profile = charge.map(|c| detect(c, &profiles));
            s.thermal = thermal;
            s.profiles = profiles.clone();
        })
    }
}
