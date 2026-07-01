//! Global configuration of FFmpeg's internal logging.
//!
//! FFmpeg keeps a single process-global logger. This module bridges it to the crate's public
//! [`LogSetting`] API: either adjust the level of FFmpeg's own stderr handler, or install a
//! Rust callback. The callback is invoked from whatever thread FFmpeg happens to log on
//! (including internal codec worker threads), so all shared state here is synchronized.

use std::ffi::{c_char, c_int};
use std::panic::{self, AssertUnwindSafe};
use std::sync::{PoisonError, RwLock};

use crate::{LogCallback, LogLevel, LogMessage, LogSetting};

unsafe extern "C" {
    /// Installs the shim's trampoline as FFmpeg's global log callback.
    fn codecpod_log_install();
    /// Restores FFmpeg's default stderr handler and sets its level threshold.
    fn codecpod_log_restore(level: c_int);
    /// Discards any half-assembled line, so switching handlers never fuses leftover fragments.
    fn codecpod_log_reset();
}

/// The user-provided callback, if one is installed. `None` means FFmpeg's default handler is
/// active. Read on every log call, written only from [`set`].
static CALLBACK: RwLock<Option<LogCallback>> = RwLock::new(None);

/// Applies a [`LogSetting`] to FFmpeg's global logger.
pub(crate) fn set(setting: LogSetting) {
    // Discard any half-assembled line in the shim so a switch never fuses leftover fragments
    // onto the next message.
    // SAFETY: plain FFI call into the shim with no aliasing or lifetime requirements.
    unsafe {
        codecpod_log_reset();
    }
    match setting {
        LogSetting::Level(level) => {
            // Drop any custom callback and restore FFmpeg's default stderr handler, which is the
            // component that actually enforces the level threshold.
            *CALLBACK.write().unwrap_or_else(PoisonError::into_inner) = None;
            // SAFETY: plain FFI setter with no aliasing or lifetime requirements.
            unsafe {
                codecpod_log_restore(level.to_av());
            }
        }
        LogSetting::Callback(cb) => {
            *CALLBACK.write().unwrap_or_else(PoisonError::into_inner) = Some(cb);
            // SAFETY: plain FFI setter; installs the shim trampoline as FFmpeg's log callback.
            unsafe {
                codecpod_log_install();
            }
        }
    }
}

/// Called by the C shim once per complete, fully rendered log line. Wraps it in a [`LogMessage`]
/// and dispatches it to the user callback.
///
/// # Safety
///
/// Called only by the shim, with `text` pointing to `len` valid bytes (a rendered log line with
/// the trailing newline already stripped, not NUL-terminated within `len`). The pointer is valid
/// only for the duration of the call. Must be thread-safe.
#[unsafe(no_mangle)]
unsafe extern "C" fn codecpod_log_dispatch(level: c_int, text: *const c_char, len: usize) {
    // SAFETY: the shim guarantees `text` points to `len` initialized bytes for this call. The
    // bytes are a rendered C string body; interpreting them as UTF-8 lossily never reads OOB.
    let bytes = unsafe { std::slice::from_raw_parts(text as *const u8, len) };
    let msg = LogMessage {
        level: LogLevel::from_av(level),
        text: String::from_utf8_lossy(bytes).into_owned(),
    };

    let guard = CALLBACK.read().unwrap_or_else(PoisonError::into_inner);
    if let Some(cb) = guard.as_ref() {
        // A panic must not unwind across the FFI boundary back into the shim / FFmpeg.
        let _ = panic::catch_unwind(AssertUnwindSafe(|| cb(&msg)));
    }
}
