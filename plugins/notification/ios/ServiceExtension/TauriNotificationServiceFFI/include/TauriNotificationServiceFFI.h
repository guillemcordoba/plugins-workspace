#pragma once

// Exported by the app crate's `#[receive_push_notification]` handler.
char *tauri_plugin_notification_receive_push(const char *notification, const char *data_dir);
void tauri_plugin_notification_free_push(char *json);
