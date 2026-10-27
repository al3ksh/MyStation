//! Boot performance from the `Microsoft-Windows-Diagnostics-Performance/Operational` log:
//! event 100 records each boot, events 101–109 name what slowed it down.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use serde::Serialize;

/// Boot-performance event IDs read from the log.
pub const EVENT_IDS: [u32; 5] = [100, 101, 102, 103, 109];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootRecord {
    pub start: DateTime<Utc>,
    /// Power-on to a usable desktop, plus the post-boot settling.
    pub total_ms: u64,
    /// Power-on to the desktop.
    pub main_path_ms: u64,
    /// Desktop to idle: startup apps and services finishing.
    pub post_boot_ms: u64,
    pub startup_apps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CulpritKind {
    App,
    Driver,
    Service,
    Device,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Culprit {
    pub name: String,
    pub kind: CulpritKind,
    /// How many of the reported boots it slowed.
    pub boots: u32,
    pub avg_delay_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootReport {
    /// Newest first.
    pub boots: Vec<BootRecord>,
    /// Worst first by total delay.
    pub culprits: Vec<Culprit>,
    /// Firmware (POST) time of the latest power-on, Task Manager's "BIOS time".
    pub firmware_ms: Option<u64>,
    /// With fast startup on, shutting down hibernates the kernel and the next power-on
    /// resumes it; Windows logs no boot event for that.
    pub fast_startup: bool,
}

/// One event: its ID, creation time and `EventData` fields.
#[derive(Debug, Default, PartialEq)]
struct RawEvent {
    id: u32,
    time: Option<DateTime<Utc>>,
    data: HashMap<String, String>,
}

fn parse_event(xml: &str) -> Option<RawEvent> {
    let mut reader = Reader::from_str(xml);
    let mut ev = RawEvent::default();
    // The element whose text is being collected and the text so far.
    let mut field: Option<String> = None;
    let mut text = String::new();
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) => {
                text.clear();
                field = match e.local_name().as_ref() {
                    "EventID" => Some(String::new()),
                    "Data" => attr(&e, "Name"),
                    _ => None,
                };
            }
            Event::Empty(e) if e.local_name().as_ref() == "TimeCreated" => {
                ev.time = attr(&e, "SystemTime").and_then(|t| t.parse().ok());
            }
            Event::Text(t) if field.is_some() => text.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if field.is_some() => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    text.push(c);
                } else if let Some(s) = resolve_predefined_entity(&r.xml10_content()) {
                    text.push_str(s);
                }
            }
            Event::End(_) => match field.take() {
                Some(name) if name.is_empty() => ev.id = text.trim().parse().ok()?,
                Some(name) => {
                    ev.data.insert(name, text.trim().to_string());
                }
                None => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    (ev.id != 0).then_some(ev)
}

fn attr(e: &BytesStart, name: &str) -> Option<String> {
    let a = e.try_get_attribute(name).ok()??;
    Some(a.normalized_value(XmlVersion::Implicit1_0).ok()?.into_owned())
}

impl RawEvent {
    fn num(&self, key: &str) -> Option<u64> {
        self.data.get(key)?.parse().ok()
    }
}

/// How many culprits the report keeps.
const MAX_CULPRITS: usize = 8;

/// Builds the report from rendered event XML. Culprits are counted only within the
/// time span of the boots kept, so old one-offs drop out.
pub fn report(events: &[String], max_boots: usize) -> BootReport {
    let parsed: Vec<RawEvent> = events.iter().filter_map(|x| parse_event(x)).collect();

    let mut boots: Vec<BootRecord> = parsed
        .iter()
        .filter(|e| e.id == 100)
        .filter_map(|e| {
            Some(BootRecord {
                start: e.data.get("BootStartTime")?.parse().ok().or(e.time)?,
                total_ms: e.num("BootTime")?,
                main_path_ms: e.num("MainPathBootTime").unwrap_or(0),
                post_boot_ms: e.num("BootPostBootTime").unwrap_or(0),
                startup_apps: e.num("BootNumStartupApps").unwrap_or(0) as u32,
            })
        })
        .collect();
    boots.sort_by_key(|b| std::cmp::Reverse(b.start));
    boots.truncate(max_boots);
    let Some(oldest) = boots.last().map(|b| b.start) else {
        return BootReport::default();
    };

    // (name, kind) → (boots, total delay)
    let mut totals: HashMap<(String, CulpritKind), (u32, u64)> = HashMap::new();
    for e in &parsed {
        let kind = match e.id {
            101 => CulpritKind::App,
            102 => CulpritKind::Driver,
            103 => CulpritKind::Service,
            109 => CulpritKind::Device,
            _ => continue,
        };
        if e.time.is_none_or(|t| t < oldest) {
            continue;
        }
        let Some(name) =
            ["FriendlyName", "Name", "FileName"].iter().filter_map(|k| e.data.get(*k)).find(|v| !v.is_empty())
        else {
            continue;
        };
        let delay = e.num("DegradationTime").unwrap_or(0);
        let entry = totals.entry((name.clone(), kind)).or_default();
        entry.0 += 1;
        entry.1 += delay;
    }
    let mut culprits: Vec<(u64, Culprit)> = totals
        .into_iter()
        .map(|((name, kind), (count, total))| {
            (total, Culprit { name, kind, boots: count, avg_delay_ms: total / u64::from(count) })
        })
        .collect();
    culprits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    BootReport { boots, culprits: culprits.into_iter().take(MAX_CULPRITS).map(|(_, c)| c).collect(), ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: u32, time: &str, data: &[(&str, &str)]) -> String {
        let fields: String = data.iter().map(|(k, v)| format!("<Data Name='{k}'>{v}</Data>")).collect();
        format!(
            "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System>\
             <Provider Name='Microsoft-Windows-Diagnostics-Performance'/><EventID>{id}</EventID>\
             <TimeCreated SystemTime='{time}'/></System><EventData>{fields}</EventData></Event>"
        )
    }

    fn boot(time: &str, total: &str) -> String {
        event(
            100,
            time,
            &[
                ("BootStartTime", time),
                ("BootTime", total),
                ("MainPathBootTime", "26077"),
                ("BootPostBootTime", "25500"),
                ("BootNumStartupApps", "15"),
            ],
        )
    }

    fn slow(id: u32, time: &str, name: &str, delay: &str) -> String {
        event(id, time, &[("Name", "x.exe"), ("FriendlyName", name), ("TotalTime", "9999"), ("DegradationTime", delay)])
    }

    #[test]
    fn reads_boots_newest_first() {
        let r = report(&[boot("2026-10-01T19:41:06.8120690Z", "51577"), boot("2026-10-05T08:00:00Z", "30000")], 10);
        assert_eq!(r.boots.len(), 2);
        assert_eq!(r.boots[0].total_ms, 30000);
        assert_eq!(
            r.boots[1],
            BootRecord {
                start: "2026-10-01T19:41:06.812069Z".parse().unwrap(),
                total_ms: 51577,
                main_path_ms: 26077,
                post_boot_ms: 25500,
                startup_apps: 15,
            }
        );
    }

    #[test]
    fn culprits_are_grouped_and_ranked_by_total_delay() {
        let r = report(
            &[
                boot("2026-10-01T19:41:00Z", "51577"),
                boot("2026-10-03T19:41:00Z", "40000"),
                slow(101, "2026-10-01T19:44:00Z", "WavesSysSvc Service Application", "758"),
                slow(101, "2026-10-03T19:44:00Z", "WavesSysSvc Service Application", "23308"),
                slow(101, "2026-10-03T19:44:01Z", "Proces hosta dla usług systemu Windows", "10346"),
                slow(103, "2026-10-03T19:44:02Z", "Windows Update", "500"),
            ],
            10,
        );
        assert_eq!(
            r.culprits,
            vec![
                Culprit {
                    name: "WavesSysSvc Service Application".into(),
                    kind: CulpritKind::App,
                    boots: 2,
                    avg_delay_ms: 12033
                },
                Culprit {
                    name: "Proces hosta dla usług systemu Windows".into(),
                    kind: CulpritKind::App,
                    boots: 1,
                    avg_delay_ms: 10346
                },
                Culprit { name: "Windows Update".into(), kind: CulpritKind::Service, boots: 1, avg_delay_ms: 500 },
            ]
        );
    }

    #[test]
    fn culprits_older_than_the_kept_boots_are_dropped() {
        let r = report(
            &[
                boot("2026-09-01T10:00:00Z", "90000"),
                boot("2026-10-01T10:00:00Z", "50000"),
                slow(102, "2026-09-01T10:03:00Z", "Old driver", "9000"),
                slow(102, "2026-10-01T10:03:00Z", "New driver", "100"),
            ],
            1,
        );
        assert_eq!(r.boots.len(), 1);
        assert_eq!(r.culprits.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["New driver"]);
        assert_eq!(r.culprits[0].kind, CulpritKind::Driver);
    }

    #[test]
    fn entities_and_blank_friendly_names() {
        let r = report(
            &[
                boot("2026-10-01T10:00:00Z", "50000"),
                event(
                    109,
                    "2026-10-01T10:01:00Z",
                    &[("FriendlyName", ""), ("Name", "Dock &amp; Hub &#x41;"), ("DegradationTime", "5")],
                ),
            ],
            5,
        );
        assert_eq!(r.culprits[0].name, "Dock & Hub A");
        assert_eq!(r.culprits[0].kind, CulpritKind::Device);
    }

    #[test]
    fn no_boots_means_an_empty_report() {
        assert_eq!(report(&[slow(101, "2026-10-01T10:01:00Z", "x", "5"), "<garbage".into()], 5), BootReport::default());
        assert_eq!(report(&[], 5), BootReport::default());
    }
}
