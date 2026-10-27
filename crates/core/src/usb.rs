//! Plugged-in USB devices and what each costs on battery: the change in discharge
//! rate between the minute before a device arrived and the minute after it settled.

use std::collections::HashMap;

use serde::Serialize;

use crate::history::HistorySample;

/// One present USB device as the PnP manager reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawUsbDevice {
    pub id: String,
    pub name: String,
    /// When it was last plugged in, Unix ms.
    pub arrived_ms: Option<i64>,
    /// Built-in devices (camera, fingerprint reader, Bluetooth) can't be unplugged.
    pub removable: bool,
    /// Selectively suspended right now rather than fully powered.
    pub suspended: bool,
    /// How it's connected, from the hub it's plugged into.
    pub link: Option<UsbLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UsbSpeed {
    /// 1.5 Mbps
    Low,
    /// 12 Mbps
    Full,
    /// 480 Mbps, USB 2
    High,
    /// 5 Gbps, USB 3
    Super,
    /// 10 Gbps or more
    SuperPlus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbLink {
    /// The speed it's running at.
    pub speed: UsbSpeed,
    /// The device can run at SuperSpeed, so a slower link is held back by something.
    pub superspeed_capable: bool,
    /// The port it's in has a USB 3 lane.
    pub superspeed_port: bool,
}

/// What keeps a USB 3 device at USB 2 speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Bottleneck {
    /// The port could do USB 3, so the cable (or an adapter) only carries USB 2.
    Cable,
    /// Plugged into a USB 2 port or hub.
    Port,
}

impl UsbLink {
    pub fn bottleneck(&self) -> Option<Bottleneck> {
        if !self.superspeed_capable || self.speed >= UsbSpeed::Super {
            return None;
        }
        Some(if self.superspeed_port { Bottleneck::Cable } else { Bottleneck::Port })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbDevice {
    pub name: String,
    pub arrived_ms: Option<i64>,
    pub suspended: bool,
    /// Extra battery draw since it was plugged in, in watts.
    pub draw_w: Option<f32>,
    pub speed: Option<UsbSpeed>,
    /// Set when it could run faster than it is.
    pub bottleneck: Option<Bottleneck>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbReport {
    /// Removable devices, biggest draw first.
    pub devices: Vec<UsbDevice>,
    /// How many built-in USB devices were left out.
    pub built_in: u32,
}

/// The minute before arrival, stopping short of the plug-in itself.
const BEFORE_MS: (i64, i64) = (-62_000, -2_000);
/// The minute after arrival, once enumeration and driver load have settled.
const AFTER_MS: (i64, i64) = (10_000, 70_000);
/// Each side needs this many on-battery samples (one a second) to count.
const MIN_SAMPLES: usize = 20;

/// Mean discharge in watts over `[from, to)`, if there are enough on-battery samples.
fn mean_discharge(samples: &[HistorySample], from: i64, to: i64) -> Option<f32> {
    let watts: Vec<f32> = samples
        .iter()
        .filter(|s| s.ts_ms >= from && s.ts_ms < to)
        .map(|s| s.battery_w)
        .collect::<Option<Vec<f32>>>()?;
    // Charging or on AC anywhere in the window spoils it.
    if watts.len() < MIN_SAMPLES || watts.iter().any(|w| *w >= 0.0) {
        return None;
    }
    Some(-watts.iter().sum::<f32>() / watts.len() as f32)
}

/// How much more the laptop drew after a device arrived at `at_ms`, to 0.1 W.
pub fn draw_change(samples: &[HistorySample], at_ms: i64) -> Option<f32> {
    let before = mean_discharge(samples, at_ms + BEFORE_MS.0, at_ms + BEFORE_MS.1)?;
    let after = mean_discharge(samples, at_ms + AFTER_MS.0, at_ms + AFTER_MS.1)?;
    Some(((after - before) * 10.0).round() / 10.0)
}

/// Builds the report. `measured` keeps each device's last measurement by ID, so it
/// outlives the half hour of history it came from.
pub fn report(raw: Vec<RawUsbDevice>, samples: &[HistorySample], measured: &mut HashMap<String, f32>) -> UsbReport {
    let built_in = raw.iter().filter(|d| !d.removable).count() as u32;
    let mut devices: Vec<UsbDevice> = raw
        .into_iter()
        .filter(|d| d.removable)
        .map(|d| {
            let draw_w = match d.arrived_ms.and_then(|at| draw_change(samples, at)) {
                Some(w) => {
                    measured.insert(d.id, w);
                    Some(w)
                }
                None => measured.get(&d.id).copied(),
            };
            UsbDevice {
                name: d.name,
                arrived_ms: d.arrived_ms,
                suspended: d.suspended,
                draw_w,
                speed: d.link.map(|l| l.speed),
                bottleneck: d.link.and_then(|l| l.bottleneck()),
            }
        })
        .collect();
    devices.sort_by(|a, b| {
        b.draw_w.unwrap_or(f32::MIN).total_cmp(&a.draw_w.unwrap_or(f32::MIN)).then_with(|| a.name.cmp(&b.name))
    });
    UsbReport { devices, built_in }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: i64 = 1_000_000_000;

    fn sample(ts_ms: i64, battery_w: Option<f32>) -> HistorySample {
        HistorySample { ts_ms, cpu_temp: None, cpu_load: None, gpu_temp: None, battery_pct: None, battery_w }
    }

    /// One sample a second from 70 s before `AT` to 80 s after, drawing `before` W
    /// until the arrival and `after` W from then on.
    fn plug_in(before: f32, after: f32) -> Vec<HistorySample> {
        (-70..80).map(|s| sample(AT + s * 1000, Some(-if s < 0 { before } else { after }))).collect()
    }

    fn device(id: &str, name: &str, arrived_ms: Option<i64>, removable: bool) -> RawUsbDevice {
        RawUsbDevice { id: id.into(), name: name.into(), arrived_ms, removable, suspended: false, link: None }
    }

    fn link(speed: UsbSpeed, superspeed_capable: bool, superspeed_port: bool) -> Option<UsbLink> {
        Some(UsbLink { speed, superspeed_capable, superspeed_port })
    }

    #[test]
    fn a_usb3_device_below_superspeed_blames_the_cable_or_the_port() {
        // A USB 3 drive at 480 Mbps on a port that has a USB 3 lane: the cable is USB 2.
        assert_eq!(link(UsbSpeed::High, true, true).unwrap().bottleneck(), Some(Bottleneck::Cable));
        // Same drive on a USB 2 port or hub.
        assert_eq!(link(UsbSpeed::High, true, false).unwrap().bottleneck(), Some(Bottleneck::Port));
        // Running at full speed, or a device that can't do more anyway.
        assert_eq!(link(UsbSpeed::Super, true, true).unwrap().bottleneck(), None);
        assert_eq!(link(UsbSpeed::SuperPlus, true, true).unwrap().bottleneck(), None);
        assert_eq!(link(UsbSpeed::High, false, true).unwrap().bottleneck(), None);
        assert_eq!(link(UsbSpeed::Full, false, false).unwrap().bottleneck(), None);
    }

    #[test]
    fn report_carries_the_link_speed_and_bottleneck() {
        let mut slow = device("USB\\A", "Portable SSD", None, true);
        slow.link = link(UsbSpeed::High, true, true);
        let mut fast = device("USB\\B", "Flash Drive", None, true);
        fast.link = link(UsbSpeed::Super, true, true);
        let r = report(vec![slow, fast, device("USB\\C", "Mouse", None, true)], &[], &mut HashMap::new());
        let got: Vec<_> = r.devices.iter().map(|d| (d.name.as_str(), d.speed, d.bottleneck)).collect();
        assert_eq!(
            got,
            [
                ("Flash Drive", Some(UsbSpeed::Super), None),
                ("Mouse", None, None),
                ("Portable SSD", Some(UsbSpeed::High), Some(Bottleneck::Cable)),
            ]
        );
    }

    #[test]
    fn draw_is_the_rise_in_discharge_after_arrival() {
        assert_eq!(draw_change(&plug_in(6.0, 8.54), AT), Some(2.5));
        assert_eq!(draw_change(&plug_in(6.0, 6.0), AT), Some(0.0));
    }

    #[test]
    fn spikes_right_around_the_plug_in_are_ignored() {
        let mut samples = plug_in(6.0, 7.0);
        for s in samples.iter_mut().filter(|s| (AT - 1000..AT + 9000).contains(&s.ts_ms)) {
            s.battery_w = Some(-40.0);
        }
        assert_eq!(draw_change(&samples, AT), Some(1.0));
    }

    #[test]
    fn no_measurement_without_enough_battery_samples() {
        // Arrival older than the history.
        assert_eq!(draw_change(&plug_in(6.0, 8.0), AT - 3_600_000), None);
        // Plugged into AC part of the time.
        let mut charging = plug_in(6.0, 8.0);
        charging[100].battery_w = Some(20.0);
        assert_eq!(draw_change(&charging, AT), None);
        // A gap in the samples.
        let sparse: Vec<HistorySample> = plug_in(6.0, 8.0).into_iter().step_by(5).collect();
        assert_eq!(draw_change(&sparse, AT), None);
        // No battery at all.
        let none: Vec<HistorySample> = (-70..80).map(|s| sample(AT + s * 1000, None)).collect();
        assert_eq!(draw_change(&none, AT), None);
    }

    #[test]
    fn report_lists_removable_devices_biggest_draw_first() {
        let mut measured = HashMap::new();
        let r = report(
            vec![
                device("USB\\A", "Mouse", Some(AT - 7_200_000), true),
                device("USB\\B", "Phone", Some(AT), true),
                device("USB\\C", "Integrated Webcam", Some(AT), false),
                device("USB\\D", "Bluetooth", None, false),
            ],
            &plug_in(6.0, 10.0),
            &mut measured,
        );
        assert_eq!(r.built_in, 2);
        assert_eq!(
            r.devices,
            vec![
                UsbDevice { name: "Phone".into(), arrived_ms: Some(AT), suspended: false, draw_w: Some(4.0), speed: None, bottleneck: None },
                UsbDevice { name: "Mouse".into(), arrived_ms: Some(AT - 7_200_000), suspended: false, draw_w: None, speed: None, bottleneck: None },
            ]
        );
        assert_eq!(measured.get("USB\\B"), Some(&4.0));
    }

    #[test]
    fn earlier_measurements_outlive_the_history() {
        let mut measured = HashMap::from([("USB\\B".to_string(), 1.5)]);
        let r = report(vec![device("USB\\B", "Dock", Some(AT), true)], &[], &mut measured);
        assert_eq!(r.devices[0].draw_w, Some(1.5));
        assert_eq!(report(vec![], &[], &mut measured), UsbReport::default());
    }
}
