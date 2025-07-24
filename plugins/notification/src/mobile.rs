// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri::{
    ipc::{Channel as TauriChannel, InvokeResponseBody},
    plugin::{PermissionState, PluginApi, PluginHandle},
    AppHandle, Emitter, Listener, Manager, Runtime,
};

use tauri_plugin_notification_models::*;

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

    let app_handle = app.clone();
    handle.run_mobile_plugin::<()>(
        "registerListener",
        RegisterListenerArgs {
            event: String::from("actionPerformed"),
            handler: TauriChannel::new(move |event| {
                if let InvokeResponseBody::Json(payload) = event {
                    let n: NotificationActionPerformedPayload =
                        serde_json::from_str(payload.as_str())?;

                    if let None = app_handle.try_state::<NotificationActionPerformedPayload>() {
                        app_handle.manage(n.clone());
                    }

                    app_handle.emit("notification://action-performed", n)?;
                };
                Ok(())
            }),
        },
    )?;
    #[cfg(feature = "push-notifications-fcm")]
    {
        let app_handle = app.clone();
        handle.run_mobile_plugin::<()>(
            "registerListener",
            RegisterListenerArgs {
                event: String::from("newFcmToken"),
                handler: TauriChannel::new(move |event| {
                    let token = match event {
                        tauri::ipc::InvokeResponseBody::Json(payload) => {
                            serde_json::from_str::<serde_json::Value>(payload.as_str())?
                                .get("token")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_owned())
                        }
                        _ => None,
                    };
                    if let Some(t) = token {
                        app_handle.emit("notification://new-fcm-token", t)?;
                    }
                    Ok(())
                }),
            },
        )?;
        let notification = Notification(handle.clone());
        if let Ok(PermissionState::Granted) = notification.permission_state() {
            app.listen("tauri://window-created", move |_| {
                if let Err(err) = notification.register_for_push_notifications() {
                    log::error!("Error registering for push notifications: {:?}.", err);
                }
            });
        }
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
///
/// You can get an instance of this type via [`NotificationExt`](crate::NotificationExt)
pub struct Notification<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Notification<R> {
    pub fn builder(&self) -> crate::NotificationBuilder<R> {
        crate::NotificationBuilder::new(self.0.clone())
    }

    pub fn request_permission(&self) -> crate::Result<PermissionState> {
        let permission_state = self
            .0
            .run_mobile_plugin::<PermissionResponse>("requestPermissions", ())
            .map(|r| r.permission_state)
            .map_err(|e| crate::Error::PluginInvoke(e))?;
        #[cfg(feature = "push-notifications-fcm")]
        {
            if let PermissionState::Granted = permission_state {
                self.register_for_push_notifications()?;
            }
        }

        Ok(permission_state)
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

    #[cfg(feature = "push-notifications-fcm")]
    pub fn fcm_project_id(&self) -> crate::Result<String> {
        let fcm_project_id_value = self
            .0
            .run_mobile_plugin::<serde_json::Value>("fcmProjectId", ())?;

        match fcm_project_id_value.get("fcmProjectId") {
            None => Err(crate::Error::GetFcmProjectIdError(String::from(
                "Error getting the FCM project_id",
            ))),
            Some(v) => match v {
                serde_json::Value::String(t) => Ok(t.clone()),
                _ => Err(crate::Error::GetFcmProjectIdError(String::from(
                    "Error getting the FCM project_id",
                ))),
            },
        }
    }

    #[cfg(feature = "push-notifications-fcm")]
    pub fn register_for_push_notifications(&self) -> crate::Result<String> {
        let token_value = self
            .0
            .run_mobile_plugin::<serde_json::Value>("registerForPushNotifications", ())?;

        match token_value.get("token") {
            None => Err(crate::Error::RegisterWithFcmError(String::from(
                "Error registering with FCM",
            ))),
            Some(v) => match v {
                serde_json::Value::String(t) => Ok(t.clone()),
                _ => Err(crate::Error::RegisterWithFcmError(String::from(
                    "Error registering with FCM",
                ))),
            },
        }
    }

    #[cfg(feature = "push-notifications-fcm")]
    pub fn get_launching_notification_action(&self) -> Option<NotificationActionPerformedPayload> {
        let payload = self.0.app().try_state::<NotificationActionPerformedPayload>()?;
        Some(payload.inner().to_owned())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionResponse {
    permission_state: PermissionState,
}
