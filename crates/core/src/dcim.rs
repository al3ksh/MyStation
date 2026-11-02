//! Dell Command | Monitor `DCIM_NumericSensor` rows -> readings.
//! Temperatures are whole °C despite `UnitModifier=-1`; fans are RPM.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DcimRow {
    pub element_name: String,
    /// `None` when the firmware leaves the reading blank (a stopped fan does).
    pub current_reading: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FanReading {
    /// "CPU", "GPU" or the original sensor name.
    pub name: String,
    pub rpm: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DcimReadings {
    pub cpu_c: Option<f32>,
    pub dimm_c: Option<f32>,
    pub skin_c: Option<f32>,
    pub fans: Vec<FanReading>,
}

fn max_temp(acc: Option<f32>, reading: i64) -> Option<f32> {
    if reading <= 0 {
        return acc;
    }
    let v = reading as f32;
    Some(acc.map_or(v, |a| a.max(v)))
}

pub fn parse_dcim(rows: &[DcimRow]) -> DcimReadings {
    let mut out = DcimReadings::default();
    for row in rows {
        let name = row.element_name.trim();
        if let Some(sensor) = name.strip_prefix("Temperature Sensor:") {
            let slot = match sensor.trim() {
                "CPU" => &mut out.cpu_c,
                s if s.starts_with("DIMM") => &mut out.dimm_c,
                "SKIN" => &mut out.skin_c,
                _ => continue,
            };
            let Some(reading) = row.current_reading else { continue };
            *slot = max_temp(*slot, reading);
        } else if let Some(fan) = name.strip_prefix("Fan Speed Sensor:") {
            // A stopped fan reads blank rather than 0.
            let reading = row.current_reading.unwrap_or(0);
            if reading < 0 {
                continue;
            }
            let name = match fan.trim() {
                "Processor Fan" => "CPU",
                "Video Fan" => "GPU",
                other => other,
            };
            out.fans.push(FanReading { name: name.into(), rpm: reading.min(u32::MAX as i64) as u32 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, v: i64) -> DcimRow {
        DcimRow { element_name: name.into(), current_reading: Some(v) }
    }

    fn blank(name: &str) -> DcimRow {
        DcimRow { element_name: name.into(), current_reading: None }
    }

    #[test]
    fn dcim_cpu_takes_max_of_duplicates() {
        let r = parse_dcim(&[row("Temperature Sensor:CPU", 58), row("Temperature Sensor:CPU", 79)]);
        assert_eq!(r.cpu_c, Some(79.0));
    }

    #[test]
    fn dcim_dimm_and_skin() {
        let r = parse_dcim(&[row("Temperature Sensor:DIMM A", 41), row("Temperature Sensor:SKIN", 33)]);
        assert_eq!(r.dimm_c, Some(41.0));
        assert_eq!(r.skin_c, Some(33.0));
    }

    #[test]
    fn dcim_fans_named() {
        let r = parse_dcim(&[
            row("Fan Speed Sensor:Processor Fan", 2400),
            row("Fan Speed Sensor:Video Fan", 2100),
            row("Fan Speed Sensor:Aux", 900),
        ]);
        assert_eq!(
            r.fans,
            vec![
                FanReading { name: "CPU".into(), rpm: 2400 },
                FanReading { name: "GPU".into(), rpm: 2100 },
                FanReading { name: "Aux".into(), rpm: 900 },
            ]
        );
    }

    #[test]
    fn dcim_non_positive_temps_skipped() {
        let r = parse_dcim(&[row("Temperature Sensor:CPU", 0), row("Temperature Sensor:SKIN", -5)]);
        assert_eq!(r.cpu_c, None);
        assert_eq!(r.skin_c, None);
    }

    #[test]
    fn dcim_stopped_fan_reports_zero() {
        let r = parse_dcim(&[row("Fan Speed Sensor:Processor Fan", 0)]);
        assert_eq!(r.fans, vec![FanReading { name: "CPU".into(), rpm: 0 }]);
    }

    #[test]
    fn dcim_blank_fan_is_stopped() {
        let r = parse_dcim(&[blank("Fan Speed Sensor:Processor Fan")]);
        assert_eq!(r.fans, vec![FanReading { name: "CPU".into(), rpm: 0 }]);
    }

    #[test]
    fn dcim_blank_reading_keeps_other_sensors() {
        let r = parse_dcim(&[
            row("Temperature Sensor:CPU", 53),
            blank("Fan Speed Sensor:Video Fan"),
            blank("Temperature Sensor:SKIN"),
        ]);
        assert_eq!(r.cpu_c, Some(53.0));
        assert_eq!(r.skin_c, None);
        assert_eq!(r.fans, vec![FanReading { name: "GPU".into(), rpm: 0 }]);
    }

    #[test]
    fn dcim_empty_gives_none() {
        assert_eq!(parse_dcim(&[]), DcimReadings::default());
    }
}
