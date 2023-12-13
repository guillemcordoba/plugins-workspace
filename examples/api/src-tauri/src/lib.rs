// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

mod cmd;
#[cfg(desktop)]
mod tray;

use std::collections::HashMap;
use std::fmt::format;
use std::thread::Thread;
use std::time::Duration;

use serde::{Serialize, Deserialize};
use tauri::{window::WindowBuilder, App, AppHandle, Manager, RunEvent, WindowUrl};

#[derive(Clone, Serialize)]
struct Reply {
    data: String,
}

pub type SetupHook = Box<dyn FnOnce(&mut App) -> Result<(), Box<dyn std::error::Error>> + Send>;
pub type OnEvent = Box<dyn FnMut(&AppHandle, RunEvent)>;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            #[cfg(desktop)]
            {
                tray::create_tray(app.handle())?;
                app.handle().plugin(tauri_plugin_cli::init())?;
                app.handle()
                    .plugin(tauri_plugin_global_shortcut::Builder::new().build())?;
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            #[cfg(mobile)]
            {
                app.handle().plugin(tauri_plugin_barcode_scanner::init())?;
            }

            let mut window_builder = WindowBuilder::new(app, "main", WindowUrl::default());
            #[cfg(desktop)]
            {
                window_builder = window_builder
                    .user_agent(&format!("Tauri API - {}", std::env::consts::OS))
                    .title("Tauri API Validation")
                    .inner_size(1000., 800.)
                    .min_inner_size(600., 400.)
                    .content_protected(true);
            }

            #[cfg(target_os = "windows")]
            {
                window_builder = window_builder
                    .transparent(true)
                    .shadow(true)
                    .decorations(false);
            }

            #[cfg(target_os = "macos")]
            {
                window_builder = window_builder.transparent(true);
            }

            let window = window_builder.build().unwrap();

            #[cfg(debug_assertions)]
            window.open_devtools();

            #[cfg(desktop)]
            std::thread::spawn(|| {
                let server = match tiny_http::Server::http("localhost:3003") {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("{}", e);
                        std::process::exit(1);
                    }
                };
                loop {
                    if let Ok(mut request) = server.recv() {
                        let mut body = Vec::new();
                        let _ = request.as_reader().read_to_end(&mut body);
                        let response = tiny_http::Response::new(
                            tiny_http::StatusCode(200),
                            request.headers().to_vec(),
                            std::io::Cursor::new(body),
                            request.body_length(),
                            None,
                        );
                        let _ = request.respond(response);
                    }
                }
            });

            app.listen_global("notification-action-performed", |event| {
                if let Ok(notification_action_performed_payload) = serde_json::from_str::<
                    tauri_plugin_notification::NotificationActionPerformedPayload,
                >(event.payload())
                {
                    println!("{notification_action_performed_payload:?}");
                }
            });

            #[cfg(mobile)]
            if let Some(launching_notification) = app.notification().get_launching_notification()? {
                println!("setup {launching_notification:?}");
            }

            let h = app.app_handle().clone();
            #[cfg(mobile)]
            tauri::async_runtime::spawn(async move {
                println!("aaaaaS {}", h.notification().register_for_push_notifications().unwrap());

            });
            Ok(())
        })
        .on_page_load(|window, _| {
            let window_ = window.clone();
            window.listen("js-event", move |event| {
                println!("got js-event with message '{:?}'", event.payload());
                let reply = Reply {
                    data: "something else".to_string(),
                };

                window_
                    .emit("rust-event", Some(reply))
                    .expect("failed to emit");
            });
        });

    #[cfg(target_os = "macos")]
    {
        builder = builder.menu(tauri::menu::Menu::default);
    }

    #[allow(unused_mut)]
    let mut app = builder
        .invoke_handler(tauri::generate_handler![
            cmd::log_operation,
            cmd::perform_request,
        ])
        .build(tauri::tauri_build_context!())
        .expect("error while building tauri application");

    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Regular);

    app.run(move |_app_handle, _event| {
        #[cfg(desktop)]
        if let RunEvent::ExitRequested { api, .. } = &_event {
            // Keep the event loop running even if all windows are closed
            // This allow us to catch system tray events when there is no window
            api.prevent_exit();
        }
    })
}

use jni::objects::JClass;
use jni::JNIEnv;
use tauri_plugin_notification::{NotificationExt, NotificationData};

#[tauri_plugin_notification::fetch_pending_notifications]
pub fn modify_push_notification(mut notification: PushNotification) -> PushNotification {
    std::thread::sleep(std::time::Duration::from_secs(5));
    //n.title = Some(String::from("AAA"));
    notification.body = String::from("AAA");
    // let mut extra: HashMap<String, Value> = HashMap::new();
    // n.extra = extra;
    notification
}

#[repr(C)]
pub struct RustByteSlice {
    pub bytes: *const u8,
    pub len: usize,
}

impl RustByteSlice {
    fn as_str(&self) -> &str {
        unsafe {
            // from_utf8_unchecked is sound because we checked in the constructor
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes, self.len))
        }
    }  
}

impl<'a> From<&'a str> for RustByteSlice {
    fn from(s: &'a str) -> Self {
        RustByteSlice{
            bytes: s.as_ptr(),
            len: s.len() as usize,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PushNotification {
    pub title: String,
    pub body: String,
}

#[no_mangle]
pub unsafe extern "C" fn notification_destroy(data: *mut RustByteSlice) {
    let _ = Box::from_raw(data);
}

#[no_mangle]
pub unsafe extern "C" fn notification_title(data: *const PushNotification) -> RustByteSlice {
    let named_data = &*data;
    RustByteSlice::from(named_data.title.as_ref())
}

#[no_mangle]
pub unsafe extern "C" fn notification_body(data: *const PushNotification) -> RustByteSlice {
    let named_data = &*data;
    RustByteSlice::from(named_data.body.as_ref())
}

#[no_mangle]
pub unsafe extern "C" fn modify_notification(notification_str: RustByteSlice) -> *mut PushNotification {
    let notification: PushNotification = serde_json::from_str(notification_str.as_str()).unwrap();

    let new_notification = modify_push_notification(notification);

   // let new_notification_str = serde_json::to_string(&new_notification).unwrap();
    //let s = &*new_notification_str;
    let boxed_data = Box::new(new_notification);
    Box::into_raw(boxed_data)
}
