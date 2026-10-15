//! The C ABI the plugin's notification service extension
//! (`ios/ServiceExtension/TauriNotificationService/TauriNotificationService.swift`) calls
//! into. The `#[receive_push_notification]` macro exports the entry point from
//! the app crate, so the extension links the app's own handler.

use std::ffi::{c_char, CStr, CString};

use crate::{NotificationData, ReceivePushNotificationContext};

pub type PushHandler =
    fn(NotificationData, ReceivePushNotificationContext) -> Option<NotificationData>;

/// Runs `handler` on the JSON-encoded notification iOS delivered and returns
/// the notification to show, JSON-encoded, or null to show none.
///
/// # Safety
/// Both arguments must be valid, NUL-terminated UTF-8 strings.
pub unsafe fn receive(
    notification: *const c_char,
    data_dir: *const c_char,
    handler: PushHandler,
) -> *mut c_char {
    let notification = unsafe { CStr::from_ptr(notification) }.to_string_lossy();
    let data_dir = unsafe { CStr::from_ptr(data_dir) }.to_string_lossy();
    let Ok(notification) = serde_json::from_str::<NotificationData>(&notification) else {
        return std::ptr::null_mut();
    };
    let context = ReceivePushNotificationContext {
        data_dir: data_dir.as_ref().into(),
    };
    handler(notification, context)
        .and_then(|n| serde_json::to_string(&n).ok())
        .and_then(|json| CString::new(json).ok())
        .map_or(std::ptr::null_mut(), CString::into_raw)
}

/// # Safety
/// `json` must come from [`receive`] and be freed only once.
pub unsafe fn free(json: *mut c_char) {
    if !json.is_null() {
        drop(unsafe { CString::from_raw(json) });
    }
}
