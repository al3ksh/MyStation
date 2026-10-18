use mystation_core::sensors::DcimRow;
use serde::Deserialize;

use super::WmiReaders;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NumericSensor {
    element_name: String,
    current_reading: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PowerSupply {
    total_output_power: Option<i64>,
}

impl WmiReaders {
    /// The plugged-in adapter's rating in watts, as the BIOS identified it.
    pub fn adapter_w(&self) -> Option<u32> {
        let rows: Vec<PowerSupply> =
            self.dcim.as_ref()?.raw_query("SELECT TotalOutputPower FROM DCIM_PowerSupply").ok()?;
        rows.into_iter().filter_map(|r| r.total_output_power).filter(|&w| w > 0).max().map(|w| w as u32)
    }

    /// Raw `DCIM_NumericSensor` rows; `None` when Dell Command | Monitor is unavailable.
    pub fn dcim(&self) -> Option<Vec<DcimRow>> {
        let rows: Vec<NumericSensor> =
            self.dcim.as_ref()?.raw_query("SELECT ElementName, CurrentReading FROM DCIM_NumericSensor").ok()?;
        Some(
            rows.into_iter()
                .map(|r| DcimRow { element_name: r.element_name, current_reading: r.current_reading })
                .collect(),
        )
    }
}
