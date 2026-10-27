//! The Windows power mode (the overlay scheme on top of the power plan) and Energy Saver.
//! Setting a mode changes it for the current power source only: Windows keeps one per source.

use mystation_core::power_mode::PowerMode;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::core::GUID;

windows::core::link!("powrprof.dll" "system" fn PowerGetActualOverlayScheme(guid: *mut GUID) -> u32);
windows::core::link!("powrprof.dll" "system" fn PowerGetEffectiveOverlayScheme(guid: *mut GUID) -> u32);
windows::core::link!("powrprof.dll" "system" fn PowerSetActiveOverlayScheme(guid: GUID) -> u32);

/// The mode chosen for the current power source; Energy Saver may override it while on.
pub fn get() -> Option<PowerMode> {
    let mut guid = GUID::zeroed();
    // SAFETY: `guid` is a valid out-pointer for the call.
    let status = unsafe { PowerGetActualOverlayScheme(&mut guid) };
    if status != 0 {
        return None;
    }
    PowerMode::from_guid(u128::from(guid))
}

pub fn set(mode: PowerMode) -> anyhow::Result<()> {
    // SAFETY: plain value argument.
    let status = unsafe { PowerSetActiveOverlayScheme(GUID::from_u128(mode.guid())) };
    anyhow::ensure!(status == 0, "Windows didn't accept the power mode (error {status})");
    Ok(())
}

/// The mode Windows actually runs at, after Energy Saver and similar overrides.
fn effective() -> Option<PowerMode> {
    let mut guid = GUID::zeroed();
    // SAFETY: `guid` is a valid out-pointer for the call.
    let status = unsafe { PowerGetEffectiveOverlayScheme(&mut guid) };
    if status != 0 {
        return None;
    }
    PowerMode::from_guid(u128::from(guid))
}

/// Energy Saver is holding Windows at best power efficiency whatever the chosen mode. Besides
/// the battery saver flag, Windows 11 can keep Energy Saver on while plugged in, which shows
/// only as an effective mode that differs from the chosen one.
pub fn energy_saver() -> bool {
    let mut s = SYSTEM_POWER_STATUS::default();
    // SAFETY: `s` is a valid out-pointer for the call.
    let flag = unsafe { GetSystemPowerStatus(&mut s) }.is_ok() && s.SystemStatusFlag & 1 != 0;
    flag || (effective() == Some(PowerMode::BestEfficiency) && get() != Some(PowerMode::BestEfficiency))
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "reads this machine's power mode"]
    fn reads_the_power_mode() {
        let mode = super::get();
        println!("{mode:?}, effective {:?}, energy saver {}", super::effective(), super::energy_saver());
        assert!(mode.is_some());
    }
}
