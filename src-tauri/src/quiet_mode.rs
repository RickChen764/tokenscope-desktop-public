use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration as StdDuration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::time::sleep;

pub const QUIET_MODE_CHANGED_EVENT: &str = "tokenscope:quiet-mode-changed";
const QUIET_MODE_FULLSCREEN_REASON: &str = "fullscreen";
const QUIET_MODE_DETECT_SECONDS: u64 = 2;

#[derive(Clone, Default)]
pub struct QuietModeRuntime {
    active: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuietModeStatus {
    pub active: bool,
    pub reason: Option<String>,
}

impl QuietModeRuntime {
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn set_active(&self, active: bool) -> bool {
        self.active
            .compare_exchange(!active, active, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn status(&self) -> QuietModeStatus {
        QuietModeStatus {
            active: self.is_active(),
            reason: self
                .is_active()
                .then(|| QUIET_MODE_FULLSCREEN_REASON.to_string()),
        }
    }
}

pub fn quiet_mode_event_payload(runtime: &QuietModeRuntime) -> QuietModeStatus {
    runtime.status()
}

pub fn spawn_fullscreen_quiet_mode_guard<R: Runtime>(app: AppHandle<R>, runtime: QuietModeRuntime) {
    if runtime.set_active(is_probably_fullscreen()) {
        let _ = app.emit(QUIET_MODE_CHANGED_EVENT, quiet_mode_event_payload(&runtime));
    }

    tauri::async_runtime::spawn(async move {
        loop {
            let active = is_probably_fullscreen();
            if runtime.set_active(active) {
                let _ = app.emit(QUIET_MODE_CHANGED_EVENT, quiet_mode_event_payload(&runtime));
            }

            sleep(StdDuration::from_secs(QUIET_MODE_DETECT_SECONDS)).await;
        }
    });
}

#[cfg(target_os = "windows")]
fn is_probably_fullscreen() -> bool {
    windows_foreground_window_is_fullscreen()
}

#[cfg(not(target_os = "windows"))]
fn is_probably_fullscreen() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn windows_foreground_window_is_fullscreen() -> bool {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, IsIconic, IsWindowVisible,
    };

    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.is_null() || IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return false;
        }

        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        ) != 0
            && GetWindowRect(hwnd, &mut rect) == 0
        {
            return false;
        }

        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return false;
        }

        let mut monitor_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };
        if GetMonitorInfoW(monitor, &mut monitor_info) == 0 {
            return false;
        }

        rect_covers_monitor_area(
            DesktopRect::from(rect),
            DesktopRect::from(monitor_info.rcMonitor),
            DesktopRect::from(monitor_info.rcWork),
        )
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DesktopRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
impl From<windows_sys::Win32::Foundation::RECT> for DesktopRect {
    fn from(rect: windows_sys::Win32::Foundation::RECT) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }
}

#[cfg(target_os = "windows")]
fn rect_covers_monitor_area(
    rect: DesktopRect,
    monitor: DesktopRect,
    work_area: DesktopRect,
) -> bool {
    const FULLSCREEN_TOLERANCE_PX: i32 = 2;

    if !rect_matches(monitor, work_area, FULLSCREEN_TOLERANCE_PX)
        && rect_matches(rect, work_area, FULLSCREEN_TOLERANCE_PX)
    {
        return false;
    }

    rect.left <= monitor.left + FULLSCREEN_TOLERANCE_PX
        && rect.top <= monitor.top + FULLSCREEN_TOLERANCE_PX
        && rect.right >= monitor.right - FULLSCREEN_TOLERANCE_PX
        && rect.bottom >= monitor.bottom - FULLSCREEN_TOLERANCE_PX
}

#[cfg(target_os = "windows")]
fn rect_matches(left: DesktopRect, right: DesktopRect, tolerance: i32) -> bool {
    (left.left - right.left).abs() <= tolerance
        && (left.top - right.top).abs() <= tolerance
        && (left.right - right.right).abs() <= tolerance
        && (left.bottom - right.bottom).abs() <= tolerance
}

#[cfg(test)]
mod tests {
    use super::{quiet_mode_event_payload, QuietModeRuntime};

    #[test]
    fn quiet_mode_runtime_reports_fullscreen_reason_when_active() {
        let runtime = QuietModeRuntime::default();

        assert!(!runtime.status().active);
        assert_eq!(runtime.status().reason, None);

        assert!(runtime.set_active(true));
        let status = runtime.status();

        assert!(status.active);
        assert_eq!(status.reason.as_deref(), Some("fullscreen"));
        assert!(!runtime.set_active(true));
        assert!(runtime.set_active(false));
        assert!(!runtime.status().active);
    }

    #[test]
    fn quiet_mode_event_payload_matches_runtime_status() {
        let runtime = QuietModeRuntime::default();
        runtime.set_active(true);

        let payload = quiet_mode_event_payload(&runtime);

        assert!(payload.active);
        assert_eq!(payload.reason.as_deref(), Some("fullscreen"));
    }
}
