//! `config.json` persistence. Loading never fails: bad data falls back to defaults.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::automation::{Automation, Routine, migrate};
use crate::dell::{ChargeCfg, validate_custom};
use crate::profile::Profiles;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub version: u32,
    pub profiles: Profiles,
    pub optimizer_warning_dismissed: bool,
    pub routines: Vec<Routine>,
    /// The fixed rules from before routines; read once to migrate, never written back.
    #[serde(rename = "automation", skip_serializing)]
    pub legacy: Option<Automation>,
    /// The first start without Dell Command | Configure opened the full window to say it is needed.
    pub setup_notice_shown: bool,
    /// Changing the thermal mode also sets the matching Windows power mode.
    pub match_power_mode: bool,
}

impl Config {
    /// Whether this start should open the full window to explain that `cctk` is missing; true once.
    pub fn take_setup_notice(&mut self, cctk_missing: bool) -> bool {
        if !cctk_missing || self.setup_notice_shown {
            return false;
        }
        self.setup_notice_shown = true;
        true
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: Profiles::default(),
            optimizer_warning_dismissed: false,
            routines: Vec::new(),
            legacy: None,
            setup_notice_shown: false,
            match_power_mode: false,
        }
    }
}

/// Anything `load` had to discard is kept as `config.json.bak`, since the next save overwrites it.
fn backup(path: &Path) {
    let _ = std::fs::copy(path, path.with_extension("json.bak"));
}

pub fn load(path: &Path) -> Config {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Config::default();
    };
    let mut cfg = match serde_json::from_str::<Config>(&text) {
        Ok(cfg) => cfg,
        Err(_) => {
            backup(path);
            return Config::default();
        }
    };
    if let Some(old) = cfg.legacy.take()
        && cfg.routines.is_empty()
    {
        cfg.routines = migrate(&old);
    }
    let mut discarded = false;
    let defaults = Profiles::default();
    for (slot, fallback) in [
        (&mut cfg.profiles.home, defaults.home),
        (&mut cfg.profiles.campus, defaults.campus),
        (&mut cfg.profiles.storage, defaults.storage),
    ] {
        if let ChargeCfg::Custom { start, stop } = *slot
            && validate_custom(start, stop).is_err()
        {
            *slot = fallback;
            discarded = true;
        }
    }
    if discarded {
        backup(path);
    }
    cfg
}

pub fn save(path: &Path, cfg: &Config) -> std::io::Result<()> {
    write_atomic(path, serde_json::to_string_pretty(cfg)?.as_bytes())
}

/// Writes `<path>.tmp`, then renames it over `path`, creating the parent directory if needed.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_notice_shows_once_and_only_when_cctk_is_missing() {
        let mut cfg = Config::default();
        assert!(!cfg.take_setup_notice(false));
        assert!(cfg.take_setup_notice(true));
        assert!(!cfg.take_setup_notice(true));
    }

    #[test]
    fn load_missing_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("config.json")), Config::default());
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut cfg = Config::default();
        cfg.profiles.home = ChargeCfg::Custom { start: 70, stop: 85 };
        cfg.optimizer_warning_dismissed = true;
        cfg.routines = vec![Routine {
            id: "a".into(),
            name: "On battery".into(),
            enabled: true,
            trigger: crate::automation::Trigger::Power { plugged: false },
            actions: crate::automation::Actions::default(),
            notify: true,
            restore: true,
        }];
        cfg.match_power_mode = true;
        save(&path, &cfg).unwrap();
        assert_eq!(load(&path), cfg);
    }

    #[test]
    fn file_without_automation_gets_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"version":1,"optimizerWarningDismissed":true}"#).unwrap();
        let cfg = load(&path);
        assert!(cfg.optimizer_warning_dismissed);
        assert_eq!(cfg.routines, vec![]);
    }

    #[test]
    fn old_rules_become_routines_and_are_not_written_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"version":1,"automation":{"thermal":{"enabled":true,"onAc":null,"onBattery":"Quiet"},"notify":false}}"#,
        )
        .unwrap();
        let cfg = load(&path);
        assert_eq!(cfg.legacy, None);
        assert_eq!(cfg.routines.len(), 1);
        assert_eq!(cfg.routines[0].id, "on-battery");
        assert!(!cfg.routines[0].notify);
        save(&path, &cfg).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("\"automation\""));
        assert_eq!(load(&path), cfg);
    }

    #[test]
    fn save_creates_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("MyStation").join("config.json");
        save(&path, &Config::default()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn corrupt_json_gives_default_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{nope").unwrap();
        assert_eq!(load(&path), Config::default());
        assert_eq!(std::fs::read_to_string(dir.path().join("config.json.bak")).unwrap(), "{nope");
    }

    #[test]
    fn invalid_custom_in_file_falls_back_per_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            r#"{"version":1,"profiles":{"home":{"kind":"Custom","start":75,"stop":77},
               "campus":{"kind":"Standard"},"storage":{"kind":"Custom","start":55,"stop":65}}}"#,
        )
        .unwrap();
        let cfg = load(&path);
        assert_eq!(cfg.profiles.home, Profiles::default().home);
        assert_eq!(cfg.profiles.storage, ChargeCfg::Custom { start: 55, stop: 65 });
    }

    #[test]
    fn invalid_custom_in_file_is_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let text = r#"{"version":1,"profiles":{"home":{"kind":"Custom","start":75,"stop":77}}}"#;
        std::fs::write(&path, text).unwrap();
        load(&path);
        assert_eq!(std::fs::read_to_string(dir.path().join("config.json.bak")).unwrap(), text);
    }

    #[test]
    fn valid_file_leaves_no_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        save(&path, &Config::default()).unwrap();
        load(&path);
        assert!(!dir.path().join("config.json.bak").exists());
    }
}
