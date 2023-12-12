// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri::{
    ipc::{Channel as TauriChannel, InvokeBody},
    plugin::{PluginApi, PluginHandle},
    AppHandle, Manager, Runtime,
};

use tauri_plugin_notification_models::*;

use serde_json::Value;

use std::collections::HashMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterListenerArgs {
    pub event: String,
    pub handler: TauriChannel,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NotificationActionPerformedPayload {
    pub action_id: String,
    pub notification: NotificationData,
}

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "app.tauri.notification";

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_notification);

// initializes the Kotlin or Swift plugin classes
pub fn init<R: Runtime, C: DeserializeOwned>(
    app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> crate::Result<Notification<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "NotificationPlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_notification)?;
    #[cfg(target_os = "android")]
    {
        let app_handle = app.clone();
        handle.run_mobile_plugin::<()>(
            "registerListener",
            RegisterListenerArgs {
                event: String::from("actionPerformed"),
                handler: TauriChannel::new(move |event| {
                    if let InvokeBody::Json(payload) = event {
                        let n: NotificationActionPerformedPayload =
                            serde_json::from_value(payload)?;
                        app_handle.emit("notification-action-performed", n)?;
                    };
                    Ok(())
                }),
            },
        )?;
    }
    Ok(Notification(handle))
}

impl<R: Runtime> crate::NotificationBuilder<R> {
    pub fn show(self) -> crate::Result<()> {
        self.handle
            .run_mobile_plugin::<i32>("show", self.data)
            .map(|_| ())
            .map_err(Into::into)
    }
}

/// Access to the notification APIs.
pub struct Notification<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Notification<R> {
    pub fn builder(&self) -> crate::NotificationBuilder<R> {
        crate::NotificationBuilder::new(self.0.clone())
    }

    pub fn get_launching_notification(
        &self,
    ) -> crate::Result<Option<NotificationActionPerformedPayload>> {
        let launching_notification = self
            .0
            .run_mobile_plugin::<Option<Value>>("getLaunchingNotification", ())?;
        match launching_notification {
            Some(notification) => {
                println!("HI {notification:?}");
                if let Some(_) = notification.get("notification") {
                    let notification: NotificationActionPerformedPayload =
                        serde_json::from_value(notification)?;
                    return Ok(Some(notification));
                } else {
                    return Ok(None);
                }
            }
            None => Ok(None),
        }
    }

    pub fn register_for_push_notifications(&self) -> crate::Result<String> {
        self.0
            .run_mobile_plugin::<String>("registerForPushNotifications", ())
            .map_err(Into::into)
    }

    pub fn get_fcm_token(&self) -> crate::Result<String> {
        self.0
            .run_mobile_plugin::<String>("getFCMToken", ())
            .map_err(Into::into)
    }

    pub fn request_permission(&self) -> crate::Result<PermissionState> {
        self.0
            .run_mobile_plugin::<PermissionResponse>("requestPermissions", ())
            .map(|r| r.permission_state)
            .map_err(Into::into)
    }

    pub fn permission_state(&self) -> crate::Result<PermissionState> {
        self.0
            .run_mobile_plugin::<PermissionResponse>("checkPermissions", ())
            .map(|r| r.permission_state)
            .map_err(Into::into)
    }

    pub fn register_action_types(&self, types: Vec<ActionType>) -> crate::Result<()> {
        let mut args = HashMap::new();
        args.insert("types", types);
        self.0
            .run_mobile_plugin("registerActionTypes", args)
            .map_err(Into::into)
    }

    pub fn remove_active(&self, notifications: Vec<i32>) -> crate::Result<()> {
        let mut args = HashMap::new();
        args.insert(
            "notifications",
            notifications
                .into_iter()
                .map(|id| {
                    let mut notification = HashMap::new();
                    notification.insert("id", id);
                    notification
                })
                .collect::<Vec<HashMap<&str, i32>>>(),
        );
        self.0
            .run_mobile_plugin("removeActive", args)
            .map_err(Into::into)
    }

    pub fn active(&self) -> crate::Result<Vec<ActiveNotification>> {
        self.0
            .run_mobile_plugin("getActive", ())
            .map_err(Into::into)
    }

    pub fn remove_all_active(&self) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("removeActive", ())
            .map_err(Into::into)
    }

    pub fn pending(&self) -> crate::Result<Vec<PendingNotification>> {
        self.0
            .run_mobile_plugin("getPending", ())
            .map_err(Into::into)
    }

    /// Cancel pending notifications.
    pub fn cancel(&self, notifications: Vec<i32>) -> crate::Result<()> {
        let mut args = HashMap::new();
        args.insert("notifications", notifications);
        self.0.run_mobile_plugin("cancel", args).map_err(Into::into)
    }

    /// Cancel all pending notifications.
    pub fn cancel_all(&self) -> crate::Result<()> {
        self.0.run_mobile_plugin("cancel", ()).map_err(Into::into)
    }

    #[cfg(target_os = "android")]
    pub fn create_channel(&self, channel: Channel) -> crate::Result<()> {
        self.0
            .run_mobile_plugin("createChannel", channel)
            .map_err(Into::into)
    }

    #[cfg(target_os = "android")]
    pub fn delete_channel(&self, id: impl Into<String>) -> crate::Result<()> {
        let mut args = HashMap::new();
        args.insert("id", id.into());
        self.0
            .run_mobile_plugin("deleteChannel", args)
            .map_err(Into::into)
    }

    #[cfg(target_os = "android")]
    pub fn list_channels(&self) -> crate::Result<Vec<Channel>> {
        self.0
            .run_mobile_plugin("listChannels", ())
            .map_err(Into::into)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionResponse {
    permission_state: PermissionState,
}
