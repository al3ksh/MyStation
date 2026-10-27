//! Reading values from HKEY_LOCAL_MACHINE.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RRF_RT_REG_QWORD, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::HSTRING;

/// A `REG_SZ` value under HKLM, or `None` when the key or value is missing.
pub fn hklm_string(key: &str, value: &str) -> Option<String> {
    let (key, value) = (HSTRING::from(key), HSTRING::from(value));
    let mut buf = [0u16; 1024];
    let mut len = size_of_val(&buf) as u32;
    // SAFETY: `buf` and `len` are live; RRF_RT_REG_SZ restricts the type to a string.
    let read = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, &key, &value, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut len))
    };
    (read == ERROR_SUCCESS).then(|| {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    })
}

/// A `REG_DWORD` value under HKLM, or `None` when the key or value is missing.
pub fn hklm_dword(key: &str, value: &str) -> Option<u32> {
    let (key, value) = (HSTRING::from(key), HSTRING::from(value));
    let mut data = 0u32;
    let mut len = size_of::<u32>() as u32;
    // SAFETY: `data` and `len` are live and sized for the REG_DWORD that RRF_RT_REG_DWORD restricts to.
    let read = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, &key, &value, RRF_RT_REG_DWORD, None, Some((&raw mut data).cast()), Some(&mut len))
    };
    (read == ERROR_SUCCESS).then_some(data)
}

/// A `REG_QWORD` value under HKLM, or `None` when the key or value is missing.
pub fn hklm_qword(key: &str, value: &str) -> Option<u64> {
    let (key, value) = (HSTRING::from(key), HSTRING::from(value));
    let mut data = 0u64;
    let mut len = size_of::<u64>() as u32;
    // SAFETY: `data` and `len` are live and sized for the REG_QWORD that RRF_RT_REG_QWORD restricts to.
    let read = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, &key, &value, RRF_RT_REG_QWORD, None, Some((&raw mut data).cast()), Some(&mut len))
    };
    (read == ERROR_SUCCESS).then_some(data)
}

/// A `REG_BINARY` value under HKLM of up to 4 KiB, or `None` when the key or value is missing.
pub fn hklm_binary(key: &str, value: &str) -> Option<Vec<u8>> {
    let (key, value) = (HSTRING::from(key), HSTRING::from(value));
    let mut buf = vec![0u8; 4096];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` and `len` are live and `len` is the buffer's size; RRF_RT_REG_BINARY restricts the type.
    let read = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, &key, &value, RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr().cast()), Some(&mut len))
    };
    (read == ERROR_SUCCESS).then(|| {
        buf.truncate(len as usize);
        buf
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_value_every_windows_install_has() {
        let root = hklm_string(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "SystemRoot").unwrap();
        assert!(root.to_ascii_lowercase().ends_with("windows"), "{root}");
    }

    #[test]
    fn missing_key_is_none() {
        assert_eq!(hklm_string(r"SOFTWARE\MyStation-test-missing", "x"), None);
        assert_eq!(hklm_dword(r"SOFTWARE\MyStation-test-missing", "x"), None);
        assert_eq!(hklm_binary(r"SOFTWARE\MyStation-test-missing", "x"), None);
        assert_eq!(hklm_qword(r"SOFTWARE\MyStation-test-missing", "x"), None);
    }

    #[test]
    fn reads_a_dword() {
        let major = hklm_dword(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentMajorVersionNumber");
        assert_eq!(major, Some(10));
    }

    #[test]
    fn reads_a_binary_value() {
        // Windows keeps its product key blob here on every install.
        let id = hklm_binary(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "DigitalProductId").unwrap();
        assert!(id.len() > 100, "{}", id.len());
    }
}
