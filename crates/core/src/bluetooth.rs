//! Battery levels of connected Bluetooth peripherals. Windows lists a device once per radio it
//! paired over (Classic and LE), so nodes sharing a container are one device.

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

/// One `BTHENUM\DEV_*` or `BTHLE\DEV_*` device node.
#[derive(Debug, Clone, PartialEq)]
pub struct RawBtNode {
    /// `DEVPKEY_Device_ContainerId`: shared by every node of one physical device.
    pub container: String,
    pub name: String,
    /// `DEVPKEY_DeviceContainer_Category`, like `Input.Mouse` or `Audio.Earbuds`.
    pub category: Option<String>,
    pub connected: bool,
    pub battery_pct: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BtKind {
    Mouse,
    Keyboard,
    Headphones,
    Speaker,
    Controller,
    Pen,
    Phone,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BtDevice {
    #[serde(skip)]
    pub container: String,
    pub name: String,
    pub kind: BtKind,
    pub battery_pct: u8,
}

/// At or below this a device is worth a notification.
pub const LOW_PCT: u8 = 15;
/// It has to charge past this before it can warn again, so a reading that wobbles
/// around the threshold doesn't notify every time.
const REARM_PCT: u8 = 25;

fn kind(category: &str) -> BtKind {
    let c = category.to_ascii_lowercase();
    if c.starts_with("input.mouse") || c.starts_with("input.touchpad") {
        BtKind::Mouse
    } else if c.starts_with("input.keyboard") {
        BtKind::Keyboard
    } else if c.starts_with("input.gaming") || c.starts_with("input.gamepad") || c.starts_with("input.joystick") {
        BtKind::Controller
    } else if c.starts_with("input.pen") {
        BtKind::Pen
    } else if c.starts_with("audio.speaker") || c.starts_with("multimedia.speaker") {
        BtKind::Speaker
    } else if c.starts_with("audio.") || c.contains("headset") || c.contains("headphone") {
        BtKind::Headphones
    } else if c.starts_with("communication.phone") {
        BtKind::Phone
    } else {
        BtKind::Other
    }
}

/// Connected devices that report a battery, emptiest first.
pub fn devices(nodes: Vec<RawBtNode>) -> Vec<BtDevice> {
    let mut groups: BTreeMap<String, Vec<RawBtNode>> = BTreeMap::new();
    for n in nodes {
        groups.entry(n.container.to_ascii_lowercase()).or_default().push(n);
    }
    let mut out: Vec<BtDevice> = groups
        .into_iter()
        .filter(|(_, g)| g.iter().any(|n| n.connected))
        .filter_map(|(container, g)| {
            let battery_pct = g.iter().filter_map(|n| n.battery_pct).min()?.min(100);
            let kind = g
                .iter()
                .filter_map(|n| n.category.as_deref())
                .map(kind)
                .find(|k| *k != BtKind::Other)
                .unwrap_or(BtKind::Other);
            let name = g.iter().map(|n| n.name.trim()).filter(|n| !n.is_empty()).min_by_key(|n| n.len())?.to_string();
            Some(BtDevice { container, name, kind, battery_pct })
        })
        .collect();
    out.sort_by(|a, b| a.battery_pct.cmp(&b.battery_pct).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Which devices have already warned about a low battery.
#[derive(Debug, Default)]
pub struct LowBattery {
    warned: HashSet<String>,
}

impl LowBattery {
    /// The devices that just ran low and haven't warned since they were last charged.
    pub fn check<'a>(&mut self, devices: &'a [BtDevice]) -> Vec<&'a BtDevice> {
        for d in devices.iter().filter(|d| d.battery_pct > REARM_PCT) {
            self.warned.remove(&d.container);
        }
        devices.iter().filter(|d| d.battery_pct <= LOW_PCT && self.warned.insert(d.container.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(container: &str, name: &str, category: Option<&str>, connected: bool, battery: Option<u8>) -> RawBtNode {
        RawBtNode {
            container: container.into(),
            name: name.into(),
            category: category.map(Into::into),
            connected,
            battery_pct: battery,
        }
    }

    fn device(container: &str, battery_pct: u8) -> BtDevice {
        BtDevice { container: container.into(), name: container.into(), kind: BtKind::Mouse, battery_pct }
    }

    #[test]
    fn categories_map_to_kinds() {
        assert_eq!(kind("Input.Mouse"), BtKind::Mouse);
        assert_eq!(kind("Input.Keyboard"), BtKind::Keyboard);
        assert_eq!(kind("Audio.Earbuds"), BtKind::Headphones);
        assert_eq!(kind("Audio.Headphone"), BtKind::Headphones);
        assert_eq!(kind("Communication.Headset.Bluetooth"), BtKind::Headphones);
        assert_eq!(kind("Audio.Speaker"), BtKind::Speaker);
        assert_eq!(kind("Input.Gaming"), BtKind::Controller);
        assert_eq!(kind("Input.Pen"), BtKind::Pen);
        assert_eq!(kind("Communication.Phone"), BtKind::Phone);
        assert_eq!(kind("Unknown"), BtKind::Other);
    }

    #[test]
    fn dual_mode_nodes_are_one_device() {
        let d = devices(vec![
            node("{111566FD}", "Buds3 Pro", Some("Communication.Headset.Bluetooth"), true, None),
            node("{111566fd}", "Buds3 Pro", Some("Audio.Earbuds"), false, Some(80)),
        ]);
        assert_eq!(
            d,
            vec![BtDevice { container: "{111566fd}".into(), name: "Buds3 Pro".into(), kind: BtKind::Headphones, battery_pct: 80 }]
        );
    }

    #[test]
    fn only_connected_devices_with_a_battery_are_listed() {
        let d = devices(vec![
            node("A", "MX Anywhere 3S", Some("Input.Mouse"), true, Some(30)),
            node("B", "Galaxy S21", Some("Communication.Phone"), true, None),
            node("C", "Keyboard", Some("Input.Keyboard"), false, Some(90)),
        ]);
        assert_eq!(d.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["MX Anywhere 3S"]);
        assert_eq!(d[0].kind, BtKind::Mouse);
    }

    #[test]
    fn emptiest_first_and_the_lower_of_two_readings() {
        let d = devices(vec![
            node("A", "Mouse", Some("Input.Mouse"), true, Some(60)),
            node("B", "Buds", Some("Audio.Earbuds"), true, Some(40)),
            node("B", "Buds", Some("Audio.Earbuds"), true, Some(20)),
            node("C", "Pen", None, true, Some(200)),
        ]);
        assert_eq!(d.iter().map(|d| (d.name.as_str(), d.battery_pct)).collect::<Vec<_>>(), [("Buds", 20), ("Mouse", 60), ("Pen", 100)]);
        assert_eq!(d[2].kind, BtKind::Other);
    }

    #[test]
    fn low_battery_warns_once_until_charged() {
        let mut low = LowBattery::default();
        assert!(low.check(&[device("A", 50)]).is_empty());
        assert_eq!(low.check(&[device("A", 15)]).len(), 1);
        assert!(low.check(&[device("A", 12)]).is_empty());
        // Wobbling just above the threshold doesn't re-arm it.
        assert!(low.check(&[device("A", 20)]).is_empty());
        assert!(low.check(&[device("A", 14)]).is_empty());
        // Disconnecting doesn't either.
        assert!(low.check(&[]).is_empty());
        assert!(low.check(&[device("A", 14)]).is_empty());
        // Charged, then run down again.
        assert!(low.check(&[device("A", 80)]).is_empty());
        assert_eq!(low.check(&[device("A", 10), device("B", 5)]).len(), 2);
    }
}
