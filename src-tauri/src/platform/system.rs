use std::os::windows::process::CommandExt;
use std::process::Command;

use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Shell::IsUserAnAdmin;

use super::CREATE_NO_WINDOW;

pub fn is_elevated() -> bool {
    // SAFETY: no arguments, no preconditions.
    unsafe { IsUserAnAdmin() }.as_bool()
}

/// Dell Optimizer fights over the same BIOS settings. Recent versions ship as "DellTechHub".
pub fn optimizer_running() -> bool {
    ["DellOptimizer", "DellTechHub"].iter().any(|svc| service_running(svc))
}

fn service_running(name: &str) -> bool {
    Command::new("sc")
        .args(["query", name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).contains("RUNNING"))
}

/// Milliseconds the system has run since it started (sleep included).
pub fn uptime_ms() -> i64 {
    // SAFETY: no arguments, no preconditions.
    unsafe { GetTickCount64() as i64 }
}
