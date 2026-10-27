//! The Windows power mode (Settings → Power → Power mode) and how it follows the thermal mode.

use serde::{Deserialize, Serialize};

use crate::dell::ThermalMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerMode {
    BestEfficiency,
    Balanced,
    BestPerformance,
}

const BEST_EFFICIENCY: u128 = 0x961cc777_2547_4f9d_8174_7d86181b8a7a;
const BEST_PERFORMANCE: u128 = 0xded574b5_45a0_4f42_8737_46345c09c238;

impl PowerMode {
    /// The overlay scheme GUID Windows uses for this mode; Balanced is the all-zero GUID.
    pub const fn guid(self) -> u128 {
        match self {
            PowerMode::BestEfficiency => BEST_EFFICIENCY,
            PowerMode::Balanced => 0,
            PowerMode::BestPerformance => BEST_PERFORMANCE,
        }
    }

    pub const fn from_guid(guid: u128) -> Option<Self> {
        match guid {
            BEST_EFFICIENCY => Some(PowerMode::BestEfficiency),
            0 => Some(PowerMode::Balanced),
            BEST_PERFORMANCE => Some(PowerMode::BestPerformance),
            _ => None,
        }
    }
}

/// The Windows power mode that goes with a thermal mode.
pub const fn for_thermal(mode: ThermalMode) -> PowerMode {
    match mode {
        ThermalMode::UltraPerformance => PowerMode::BestPerformance,
        ThermalMode::Optimized => PowerMode::Balanced,
        ThermalMode::Cool | ThermalMode::Quiet => PowerMode::BestEfficiency,
    }
}

/// Windows keeps a power mode per power source and switches to the other one when the laptop
/// is plugged in or out. This notices the switch so the matching mode can be set back.
#[derive(Default)]
pub struct SourceWatch {
    on_ac: Option<bool>,
}

impl SourceWatch {
    /// The mode to set now, if the source just changed and the modes are matched.
    pub fn observe(&mut self, on_ac: bool, matched: bool, thermal: Option<ThermalMode>) -> Option<PowerMode> {
        let changed = self.on_ac.replace(on_ac).is_some_and(|before| before != on_ac);
        if changed && matched { thermal.map(for_thermal) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_thermal_mode_has_a_windows_mode() {
        assert_eq!(for_thermal(ThermalMode::UltraPerformance), PowerMode::BestPerformance);
        assert_eq!(for_thermal(ThermalMode::Optimized), PowerMode::Balanced);
        assert_eq!(for_thermal(ThermalMode::Cool), PowerMode::BestEfficiency);
        assert_eq!(for_thermal(ThermalMode::Quiet), PowerMode::BestEfficiency);
    }

    #[test]
    fn guids_round_trip() {
        for m in [PowerMode::BestEfficiency, PowerMode::Balanced, PowerMode::BestPerformance] {
            assert_eq!(PowerMode::from_guid(m.guid()), Some(m));
        }
        assert_eq!(PowerMode::BestPerformance.guid(), 0xded574b5_45a0_4f42_8737_46345c09c238);
        assert_eq!(PowerMode::BestEfficiency.guid(), 0x961cc777_2547_4f9d_8174_7d86181b8a7a);
        assert_eq!(PowerMode::Balanced.guid(), 0);
        assert_eq!(PowerMode::from_guid(0x1234), None);
    }

    #[test]
    fn first_reading_of_the_power_source_sets_nothing() {
        let mut f = SourceWatch::default();
        assert_eq!(f.observe(true, true, Some(ThermalMode::UltraPerformance)), None);
    }

    #[test]
    fn plugging_in_or_out_restores_the_matching_mode() {
        let mut f = SourceWatch::default();
        f.observe(true, true, Some(ThermalMode::UltraPerformance));
        assert_eq!(f.observe(true, true, Some(ThermalMode::UltraPerformance)), None);
        assert_eq!(f.observe(false, true, Some(ThermalMode::UltraPerformance)), Some(PowerMode::BestPerformance));
        assert_eq!(f.observe(true, true, Some(ThermalMode::Quiet)), Some(PowerMode::BestEfficiency));
    }

    #[test]
    fn nothing_follows_when_off_or_the_thermal_mode_is_unknown() {
        let mut f = SourceWatch::default();
        f.observe(true, false, Some(ThermalMode::Optimized));
        assert_eq!(f.observe(false, false, Some(ThermalMode::Optimized)), None);
        assert_eq!(f.observe(true, true, None), None);
    }
}
