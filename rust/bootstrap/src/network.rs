//! The cost of the current Internet path, for clients that hold back downloads on paid networks.

/// Reads the current path from a fresh Network.framework path monitor. `Some(true)` means a
/// satisfied path that is neither expensive (cellular or a hotspot) nor constrained (Low Data
/// Mode). `None` means no answer within two seconds.
#[cfg(target_os = "macos")]
#[must_use]
pub fn unmetered() -> Option<bool> {
    use std::ffi::c_void;
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::sync_channel(1);
    let handler = block2::RcBlock::new(move |path: *mut c_void| {
        // SAFETY: the monitor passes a live path object for the duration of the handler.
        let answer = unsafe {
            unmetered_path(
                ffi::nw_path_get_status(path) == ffi::SATISFIED,
                ffi::nw_path_is_expensive(path),
                ffi::nw_path_is_constrained(path),
            )
        };
        // Only the first update matters; later ones find the channel full or closed.
        let _ = tx.try_send(answer);
    });
    // SAFETY: the monitor is created, configured and started before use and released once
    // after cancel. The framework copies the handler block, so it outlives this frame.
    unsafe {
        let monitor = ffi::nw_path_monitor_create();
        if monitor.is_null() {
            return None;
        }
        ffi::nw_path_monitor_set_queue(monitor, ffi::dispatch_get_global_queue(0, 0));
        ffi::nw_path_monitor_set_update_handler(monitor, &handler);
        ffi::nw_path_monitor_start(monitor);
        let answer = rx.recv_timeout(Duration::from_secs(2)).ok();
        ffi::nw_path_monitor_cancel(monitor);
        ffi::nw_release(monitor);
        answer
    }
}

/// Only a satisfied path that is neither expensive nor constrained counts as unmetered.
#[cfg(any(target_os = "macos", test))]
fn unmetered_path(satisfied: bool, expensive: bool, constrained: bool) -> bool {
    satisfied && !expensive && !constrained
}

#[cfg(target_os = "macos")]
mod ffi {
    use std::ffi::{c_int, c_void};

    /// `nw_path_status_satisfied`.
    pub(super) const SATISFIED: c_int = 1;

    #[link(name = "Network", kind = "framework")]
    extern "C" {
        pub(super) fn nw_path_monitor_create() -> *mut c_void;
        pub(super) fn nw_path_monitor_set_queue(monitor: *mut c_void, queue: *mut c_void);
        pub(super) fn nw_path_monitor_set_update_handler(
            monitor: *mut c_void,
            handler: &block2::Block<dyn Fn(*mut c_void)>,
        );
        pub(super) fn nw_path_monitor_start(monitor: *mut c_void);
        pub(super) fn nw_path_monitor_cancel(monitor: *mut c_void);
        pub(super) fn nw_path_get_status(path: *mut c_void) -> c_int;
        pub(super) fn nw_path_is_expensive(path: *mut c_void) -> bool;
        pub(super) fn nw_path_is_constrained(path: *mut c_void) -> bool;
        pub(super) fn nw_release(object: *mut c_void);
    }

    // libdispatch is part of libSystem, which every macOS binary links.
    extern "C" {
        pub(super) fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut c_void;
    }
}

#[cfg(test)]
mod tests {
    use super::unmetered_path;

    #[test]
    fn only_a_cheap_satisfied_path_is_unmetered() {
        assert!(unmetered_path(true, false, false));
        assert!(!unmetered_path(false, false, false));
        assert!(!unmetered_path(true, true, false));
        assert!(!unmetered_path(true, false, true));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_path_monitor_answers() {
        assert!(super::unmetered().is_some());
    }
}
