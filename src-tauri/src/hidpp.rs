//! Battery level of a Logitech wireless mouse, read straight from the USB receiver (or the
//! mouse itself when it is plugged in) with Logitech's HID++ 2.0 protocol. Read-only: the
//! only messages sent are "which features do you have" and "what is the battery level".
//! It does not need Logitech G HUB and shares the device with it when it is running.
#![cfg(windows)]

use std::cell::Cell;
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetHidGuid, HidD_GetPreparsedData, HidP_GetCaps, HIDP_CAPS, HIDP_STATUS_SUCCESS,
    PHIDP_PREPARSED_DATA,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
    /// Where the reading came from, for the trace log: HID++ feature, device index, raw reply.
    pub feature: u16,
    pub device: u8,
    pub raw: [u8; 4],
}

const LONG_REPORT: u8 = 0x11;
/// How a request ended. The difference matters: `Refused` is the device saying no, while
/// `Silent` says nothing about it (asleep, reconnecting, or simply slow).
enum Answer {
    Reply([u8; 16]),
    Refused,
    Silent,
}

/// A device that went quiet mid-conversation. Its late replies would be mistaken for answers
/// to later questions, so the whole reading is abandoned instead.
struct Unsure;
const ROOT: u8 = 0x00;
const FEATURE_NAME_TYPE: u16 = 0x0005;
const FEATURE_BATTERY_STATUS: u16 = 0x1000;
const FEATURE_BATTERY_VOLTAGE: u16 = 0x1001;
const FEATURE_UNIFIED_BATTERY: u16 = 0x1004;

/// An open HID++ channel (the vendor-defined "long report" collection of a Logitech device).
struct Channel {
    handle: HANDLE,
    event: HANDLE,
    input_len: usize,
    /// Tag of the last request. Every request gets a new one, so a late reply to an earlier
    /// request, or a reply meant for other software such as G HUB, is not taken for ours.
    tag: Cell<u8>,
}

impl Drop for Channel {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
            let _ = CloseHandle(self.event);
        }
    }
}

/// Device interface paths of every connected Logitech HID collection.
fn logitech_paths() -> Vec<Vec<u16>> {
    let mut paths = Vec::new();
    unsafe {
        let guid = HidD_GetHidGuid();
        let Ok(set) = SetupDiGetClassDevsW(Some(&guid), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE) else {
            return paths;
        };
        for index in 0..512 {
            let mut iface = SP_DEVICE_INTERFACE_DATA { cbSize: core::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInterfaces(set, None, &guid, index, &mut iface).is_err() {
                break;
            }
            let mut needed = 0u32;
            let _ = SetupDiGetDeviceInterfaceDetailW(set, &iface, None, 0, Some(&mut needed), None);
            if needed < 8 {
                continue;
            }
            // u64 storage keeps the buffer aligned for the detail structure.
            let mut buffer = vec![0u64; (needed as usize + 7) / 8];
            let detail = buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = core::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(set, &iface, Some(detail), needed, None, None).is_err() {
                continue;
            }
            let start = core::ptr::addr_of!((*detail).DevicePath) as *const u16;
            let max = (needed as usize - 4) / 2;
            let len = (0..max).take_while(|&i| *start.add(i) != 0).count();
            let path = core::slice::from_raw_parts(start, len);
            if String::from_utf16_lossy(path).to_ascii_lowercase().contains("vid_046d") {
                let mut owned = path.to_vec();
                owned.push(0);
                paths.push(owned);
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    paths
}

/// Open a path if it is a HID++ long-report collection.
fn open(path: &[u16]) -> Option<Channel> {
    unsafe {
        let handle = CreateFileW(
            PCWSTR(path.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            None,
        )
        .ok()?;
        let mut caps = HIDP_CAPS::default();
        let mut parsed = PHIDP_PREPARSED_DATA::default();
        let known = HidD_GetPreparsedData(handle, &mut parsed) && {
            let ok = HidP_GetCaps(parsed, &mut caps) == HIDP_STATUS_SUCCESS;
            let _ = HidD_FreePreparsedData(parsed);
            ok
        };
        let vendor_page = caps.UsagePage == 0xFF00 || caps.UsagePage == 0xFF43;
        let event = if known && vendor_page && caps.OutputReportByteLength == 20 { CreateEventW(None, true, false, PCWSTR::null()).ok() } else { None };
        match event {
            Some(event) => Some(Channel { handle, event, input_len: (caps.InputReportByteLength as usize).max(20), tag: Cell::new(start_tag()) }),
            None => {
                let _ = CloseHandle(handle);
                None
            }
        }
    }
}

impl Channel {
    /// One overlapped read or write with a time limit. Returns the bytes transferred.
    unsafe fn transfer(&self, write: bool, buffer: &mut [u8], limit: Duration) -> Option<u32> {
        let mut overlapped = OVERLAPPED { hEvent: self.event, ..Default::default() };
        let mut done = 0u32;
        let started = if write {
            WriteFile(self.handle, Some(buffer), None, Some(&mut overlapped))
        } else {
            ReadFile(self.handle, Some(buffer), None, Some(&mut overlapped))
        };
        if let Err(error) = started {
            if error.code() != ERROR_IO_PENDING.to_hresult() {
                return None;
            }
            if WaitForSingleObject(self.event, limit.as_millis() as u32) != WAIT_OBJECT_0 {
                // Stop the request and wait until Windows has let go of `overlapped`.
                let _ = CancelIoEx(self.handle, Some(&overlapped));
                let _ = GetOverlappedResult(self.handle, &overlapped, &mut done, true);
                return None;
            }
        }
        GetOverlappedResult(self.handle, &overlapped, &mut done, false).ok()?;
        Some(done)
    }

    /// Send a HID++ 2.0 request and wait for its answer.
    fn request(&self, device: u8, feature_index: u8, function: u8, params: &[u8]) -> Answer {
        // Tags run 1 to 15; 0 is what devices use for their own announcements.
        let tag = self.tag.get() % 15 + 1;
        self.tag.set(tag);
        let address = (function << 4) | tag;
        let mut out = [0u8; 20];
        out[0] = LONG_REPORT;
        out[1] = device;
        out[2] = feature_index;
        out[3] = address;
        out[4..4 + params.len()].copy_from_slice(params);
        unsafe {
            if self.transfer(true, &mut out, Duration::from_millis(300)).is_none() {
                return Answer::Silent;
            }
            let deadline = Instant::now() + Duration::from_millis(450);
            let mut input = vec![0u8; self.input_len];
            loop {
                let Some(left) = deadline.checked_duration_since(Instant::now()) else { return Answer::Silent };
                let Some(got) = self.transfer(false, &mut input, left) else { return Answer::Silent };
                if (got as usize) < 7 || input[0] != LONG_REPORT || input[1] != device {
                    continue; // something else on the same channel, such as a notification
                }
                if input[2] == feature_index && input[3] == address {
                    let mut reply = [0u8; 16];
                    reply.copy_from_slice(&input[4..20]);
                    return Answer::Reply(reply);
                }
                if input[2] == 0xFF && input[3] == feature_index && input[4] == address {
                    return Answer::Refused; // HID++ 2.0 error reply
                }
            }
        }
    }

    /// Index of a feature on the device: `Ok(None)` when the device says it does not have it.
    /// Asked twice, and both answers must agree, because the answer does not name the feature
    /// it is about and a wrong index would make some unrelated reply look like a battery level.
    fn feature(&self, device: u8, id: u16) -> Result<Option<u8>, Unsure> {
        let ask = || match self.request(device, ROOT, 0, &id.to_be_bytes()) {
            Answer::Reply(r) => Ok(r[0]),
            Answer::Refused => Ok(0),
            Answer::Silent => Err(Unsure),
        };
        let (first, second) = (ask()?, ask()?);
        if first != second {
            return Err(Unsure);
        }
        Ok((first != 0).then_some(first))
    }

    /// One reading from a battery feature, or `Unsure` if the device did not answer plainly.
    fn read(&self, device: u8, index: u8, function: u8) -> Result<[u8; 16], Unsure> {
        match self.request(device, index, function, &[]) {
            Answer::Reply(r) => Ok(r),
            _ => Err(Unsure),
        }
    }

    /// Battery of the device at `device`, and whether that device is a mouse or trackball
    /// (`None` when the device did not say what it is). Any doubt gives no reading at all.
    fn battery(&self, device: u8) -> Result<Option<(Battery, Option<bool>)>, Unsure> {
        let raw = |r: &[u8; 16]| [r[0], r[1], r[2], r[3]];
        let battery = if let Some(index) = self.feature(device, FEATURE_UNIFIED_BATTERY)? {
            // [state of charge %, level, charging status, external power]. Status 0 is
            // discharging, 1 to 3 are charging, slow charging and full on the charger; external
            // power 1 means a cable or charging pad is supplying power.
            let r = self.read(device, index, 1)?;
            // Anything outside what this feature can say is not a battery reading.
            if r[0] > 100 || !matches!(r[1], 0 | 1 | 2 | 4 | 8) || r[2] > 5 || r[3] > 1 || (r[0] == 0 && r[1] == 0) {
                return Err(Unsure);
            }
            // Mice that do not report a percentage send 0 with a coarse level instead (8 full, 4 good, 2 low, 1 critical).
            let percent = match (r[0], r[1]) {
                (0, 8) => 90,
                (0, 4) => 50,
                (0, 2) => 20,
                (0, 1) => 5,
                (percent, _) => percent,
            };
            Battery { percent, charging: matches!(r[2], 1..=3) || r[3] == 1, feature: FEATURE_UNIFIED_BATTERY, device, raw: raw(&r) }
        } else if let Some(index) = self.feature(device, FEATURE_BATTERY_STATUS)? {
            let r = self.read(device, index, 0)?; // [level %, next level %, status]
            if r[0] > 100 || r[1] > 100 || r[2] > 7 {
                return Err(Unsure);
            }
            Battery { percent: r[0], charging: matches!(r[2], 1..=4), feature: FEATURE_BATTERY_STATUS, device, raw: raw(&r) }
        } else if let Some(index) = self.feature(device, FEATURE_BATTERY_VOLTAGE)? {
            let r = self.read(device, index, 0)?; // [millivolts (2 bytes), flags]
            let mv = u16::from_be_bytes([r[0], r[1]]);
            if !(3000..=4500).contains(&mv) {
                return Err(Unsure);
            }
            Battery { percent: percent_from_millivolts(mv), charging: r[2] & 0x80 != 0, feature: FEATURE_BATTERY_VOLTAGE, device, raw: raw(&r) }
        } else {
            return Ok(None);
        };
        // Device type: 3 is a mouse and 5 a trackball; keyboards, headsets and the like are other values.
        let pointer = match self.feature(device, FEATURE_NAME_TYPE)? {
            Some(index) => Some(matches!(self.read(device, index, 2)?[0], 3 | 5)),
            None => None,
        };
        Ok(Some((battery, pointer)))
    }
}

/// Where the request tags start, different on each run so two programs rarely share one.
fn start_tag() -> u8 {
    (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()) % 15) as u8
}

/// Approximate charge of a single lithium cell from its voltage (for mice that report volts).
fn percent_from_millivolts(mv: u16) -> u8 {
    const CURVE: [(u16, u8); 13] = [
        (4186, 100), (4067, 90), (3989, 80), (3922, 70), (3859, 60), (3811, 50), (3778, 40),
        (3751, 30), (3717, 20), (3671, 10), (3646, 5), (3579, 2), (3500, 0),
    ];
    if mv >= CURVE[0].0 {
        return 100;
    }
    for pair in CURVE.windows(2) {
        let ((high_mv, high), (low_mv, low)) = (pair[0], pair[1]);
        if mv >= low_mv {
            let span = (high_mv - low_mv) as u32;
            return (low as u32 + (mv - low_mv) as u32 * (high - low) as u32 / span) as u8;
        }
    }
    0
}

/// Look for a Logitech wireless mouse and read its battery. Only a mouse counts: a Logitech
/// keyboard or headset on the same PC is never shown in its place. A device that will not say
/// what it is is used only when no mouse answered. `None` when nothing answered (asleep or absent).
pub fn mouse_battery() -> Option<Battery> {
    let mut unknown = None;
    for path in logitech_paths() {
        let Some(channel) = open(&path) else { continue };
        // 1: the device paired to a Lightspeed receiver; 0xFF: a mouse connected by cable; 2: a second paired device.
        for device in [0x01, 0xFF, 0x02] {
            match channel.battery(device) {
                Ok(Some((battery, Some(true)))) => return Some(battery),
                Ok(Some((battery, None))) => unknown = unknown.or(Some(battery)),
                _ => {}
            }
        }
    }
    unknown
}

#[cfg(test)]
mod tests {
    use super::percent_from_millivolts;

    #[test]
    fn voltage_curve_is_monotonic_and_bounded() {
        assert_eq!(percent_from_millivolts(4300), 100);
        assert_eq!(percent_from_millivolts(3400), 0);
        assert_eq!(percent_from_millivolts(3811), 50);
        let mut last = 0;
        for mv in 3500..4200 {
            let p = percent_from_millivolts(mv);
            assert!(p >= last && p <= 100);
            last = p;
        }
    }
}
