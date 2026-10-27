//! Windows-specific readers: cctk process, WMI (battery + Dell Command | Monitor), CPU, RAM, system, shell.

mod cctk_exec;
pub mod console;
mod cpu_times;
mod energy_meter;
pub mod event_log;
pub mod file_dialog;
pub mod gpu_apps;
mod gpu_power;
mod mem;
mod nvml;
pub mod powercfg;
pub mod registry;
mod shell;
mod smbios;
pub mod storage;
mod system;
pub mod usb;
pub mod bluetooth;
mod wmi_battery;
mod wmi_dcim;

pub use cctk_exec::ExeCctkRunner;
pub use cpu_times::cpu_times;
pub use energy_meter::CpuPowerReader;
pub use gpu_power::{DevicePower, NvidiaAdapter, nvidia_adapter, nvidia_power_state};
pub use mem::mem;
pub use nvml::{GpuReader, Nvml};
pub use shell::{light_taskbar, tray_icon_size};
pub use smbios::device_info;
pub use system::{is_elevated, optimizer_running};

use wmi::WMIConnection;

/// `CREATE_NO_WINDOW`: child console processes must not flash a window.
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// WMI connections are `!Send`: create and use this on the poller thread only.
pub struct WmiReaders {
    root_wmi: WMIConnection,
    dcim: Option<WMIConnection>,
}

impl WmiReaders {
    pub fn new() -> anyhow::Result<Self> {
        let root_wmi = WMIConnection::with_namespace_path(r"ROOT\WMI")?;
        // Dell Command | Monitor may be missing; sensors then degrade to "unavailable".
        let dcim = WMIConnection::with_namespace_path(r"ROOT\DCIM\SYSMAN").ok();
        Ok(Self { root_wmi, dcim })
    }

    pub fn dcm_available(&self) -> bool {
        self.dcim.is_some()
    }
}
