//! NVML loaded at runtime (no link-time dependency on the NVIDIA driver),
//! gated by the PnP power state so polling never wakes a sleeping dGPU.

use std::ffi::c_void;

use libloading::Library;
use mystation_core::sensors::GpuSnapshot;

use super::gpu_power::{DevicePower, nvidia_power_state};

type Device = *mut c_void;
type InitFn = unsafe extern "C" fn() -> i32;
type HandleFn = unsafe extern "C" fn(u32, *mut Device) -> i32;
type TempFn = unsafe extern "C" fn(Device, u32, *mut u32) -> i32;
type UtilFn = unsafe extern "C" fn(Device, *mut Utilization) -> i32;
type PowerFn = unsafe extern "C" fn(Device, *mut u32) -> i32;
type ClockFn = unsafe extern "C" fn(Device, u32, *mut u32) -> i32;
type MemoryFn = unsafe extern "C" fn(Device, *mut Memory) -> i32;

const NVML_SUCCESS: i32 = 0;
const NVML_TEMPERATURE_GPU: u32 = 0;
const NVML_CLOCK_GRAPHICS: u32 = 0;
const NVML_CLOCK_MEM: u32 = 2;
const MIB: u64 = 1024 * 1024;

#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: u32,
    memory: u32,
}

/// `nvmlMemory_t`, in bytes.
#[repr(C)]
#[derive(Default)]
struct Memory {
    total: u64,
    free: u64,
    used: u64,
}

pub struct Nvml {
    shutdown: InitFn,
    temperature: TempFn,
    utilization: UtilFn,
    /// Missing on drivers and boards that don't report board power.
    power: Option<PowerFn>,
    clock: Option<ClockFn>,
    memory: Option<MemoryFn>,
    device: Device,
    // Keeps the function pointers above valid; dropped last.
    _lib: Library,
}

impl Nvml {
    pub fn load() -> Option<Self> {
        // SAFETY: nvml.dll is the NVIDIA driver's library; the symbol types match the NVML C API.
        unsafe {
            let lib = Library::new("nvml.dll").ok()?;
            let init: InitFn = *lib.get(b"nvmlInit_v2\0").ok()?;
            let shutdown: InitFn = *lib.get(b"nvmlShutdown\0").ok()?;
            let handle: HandleFn = *lib.get(b"nvmlDeviceGetHandleByIndex_v2\0").ok()?;
            let temperature: TempFn = *lib.get(b"nvmlDeviceGetTemperature\0").ok()?;
            let utilization: UtilFn = *lib.get(b"nvmlDeviceGetUtilizationRates\0").ok()?;
            let power: Option<PowerFn> = lib.get(b"nvmlDeviceGetPowerUsage\0").ok().map(|f| *f);
            let clock: Option<ClockFn> = lib.get(b"nvmlDeviceGetClockInfo\0").ok().map(|f| *f);
            let memory: Option<MemoryFn> = lib.get(b"nvmlDeviceGetMemoryInfo\0").ok().map(|f| *f);
            if init() != NVML_SUCCESS {
                return None;
            }
            let mut device: Device = std::ptr::null_mut();
            if handle(0, &mut device) != NVML_SUCCESS {
                shutdown();
                return None;
            }
            Some(Self { shutdown, temperature, utilization, power, clock, memory, device, _lib: lib })
        }
    }

    /// Temperature and utilization, plus the clocks and memory where the driver reports them.
    pub fn read(&self) -> Option<GpuSnapshot> {
        let mut temp = 0;
        let mut util = Utilization::default();
        // SAFETY: `device` came from a successful nvmlDeviceGetHandleByIndex_v2 and NVML is initialized.
        unsafe {
            if (self.temperature)(self.device, NVML_TEMPERATURE_GPU, &mut temp) != NVML_SUCCESS
                || (self.utilization)(self.device, &mut util) != NVML_SUCCESS
            {
                return None;
            }
        }
        let (vram_used_mb, vram_total_mb) = self.memory_mb().unzip();
        Some(GpuSnapshot::Active {
            temp_c: temp,
            load_pct: util.gpu,
            clock_mhz: self.clock_mhz(NVML_CLOCK_GRAPHICS),
            mem_clock_mhz: self.clock_mhz(NVML_CLOCK_MEM),
            vram_used_mb,
            vram_total_mb,
        })
    }

    fn clock_mhz(&self, kind: u32) -> Option<u32> {
        let mut mhz = 0;
        // SAFETY: as in `read`; the symbol type matches nvmlDeviceGetClockInfo.
        let status = unsafe { (self.clock?)(self.device, kind, &mut mhz) };
        (status == NVML_SUCCESS).then_some(mhz)
    }

    /// `(used, total)` dedicated memory in MiB.
    fn memory_mb(&self) -> Option<(u32, u32)> {
        let mut m = Memory::default();
        // SAFETY: as in `read`; the symbol type matches nvmlDeviceGetMemoryInfo (the v1 struct).
        let status = unsafe { (self.memory?)(self.device, &mut m) };
        (status == NVML_SUCCESS && m.total > 0).then_some(((m.used / MIB) as u32, (m.total / MIB) as u32))
    }

    /// Board power draw in watts.
    pub fn power_w(&self) -> Option<f32> {
        let mut mw = 0;
        // SAFETY: as in `read`; the symbol type matches nvmlDeviceGetPowerUsage.
        let status = unsafe { (self.power?)(self.device, &mut mw) };
        (status == NVML_SUCCESS).then(|| mw as f32 / 1000.0)
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        // SAFETY: NVML was initialized in `load`.
        unsafe {
            (self.shutdown)();
        }
    }
}

pub struct GpuReader {
    nvml: Option<Nvml>,
    unavailable: bool,
}

impl Default for GpuReader {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuReader {
    pub fn new() -> Self {
        Self { nvml: None, unavailable: false }
    }

    /// Close the NVML session (no window open): an idle session must not keep the card powered.
    /// The next `read` loads NVML again, so a driver installed meanwhile is picked up.
    pub fn release(&mut self) {
        self.nvml = None;
        self.unavailable = false;
    }

    pub fn read(&mut self) -> GpuSnapshot {
        match nvidia_power_state() {
            DevicePower::NotFound => GpuSnapshot::Unavailable,
            DevicePower::Sleeping => {
                // Release NVML so an open session cannot keep the card powered.
                self.nvml = None;
                GpuSnapshot::Asleep
            }
            DevicePower::D0 => {
                if self.unavailable {
                    return GpuSnapshot::Unavailable;
                }
                if self.nvml.is_none() {
                    self.nvml = Nvml::load();
                    self.unavailable = self.nvml.is_none();
                }
                self.nvml.as_ref().and_then(Nvml::read).unwrap_or(GpuSnapshot::Unavailable)
            }
        }
    }

    /// Power draw for the snapshot `read` just returned: none while asleep, and only an
    /// open session is asked, so this never wakes the card either.
    pub fn power_w(&self, snapshot: &GpuSnapshot) -> Option<f32> {
        match snapshot {
            GpuSnapshot::Asleep => Some(0.0),
            GpuSnapshot::Unavailable => None,
            GpuSnapshot::Active { .. } => self.nvml.as_ref()?.power_w(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_closes_the_session_and_retries_loading() {
        let mut reader = GpuReader { nvml: None, unavailable: true };
        reader.release();
        assert!(reader.nvml.is_none());
        assert!(!reader.unavailable);
    }

    #[test]
    fn a_sleeping_card_draws_nothing_and_an_unread_one_is_unknown() {
        let reader = GpuReader::new();
        assert_eq!(reader.power_w(&GpuSnapshot::Asleep), Some(0.0));
        assert_eq!(reader.power_w(&GpuSnapshot::Unavailable), None);
        assert_eq!(reader.power_w(&GpuSnapshot::active(50, 0)), None);
    }
}
