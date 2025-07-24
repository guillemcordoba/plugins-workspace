// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT
use regex::Regex;
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const COMMANDS: &[&str] = &[
    "notify",
    "request_permission",
    "is_permission_granted",
    "register_action_types",
    "register_listener",
    "cancel",
    "get_pending",
    "remove_active",
    "get_active",
    "check_permissions",
    "show",
    "batch",
    "list_channels",
    "delete_channel",
    "create_channel",
    "permission_state",
    "get_launching_notification_action",
];

fn google_services_path() -> Result<Option<PathBuf>, String> {
    let Ok(android_project_path_str) = std::env::var("TAURI_ANDROID_PROJECT_PATH") else {
        return Err(String::from(
            "TAURI_ANDROID_PROJECT_PATH is not defined: are you building with tauri build?",
        ));
    };
    println!(
        "cargo:rerun-if-changed={}/google-services.json",
        android_project_path_str
    );
    println!(
        "cargo:rerun-if-changed={}/app/google-services.json",
        android_project_path_str
    );
    let android_project_path = PathBuf::from(android_project_path_str);

    if fs::exists(android_project_path.join("google-services.json"))
        .map_err(|err| format!("{err:?}"))?
    {
        Ok(Some(android_project_path.join("google-services.json")))
    } else if fs::exists(
        android_project_path
            .join("app")
            .join("google-services.json"),
    )
    .map_err(|err| format!("{err:?}"))?
    {
        Ok(Some(
            android_project_path
                .join("app")
                .join("google-services.json"),
        ))
    } else {
        println!(
            "cargo::warning={}",
            "No google-services.json file was not found. To enable push notifications in android, make sure to download the google-services.json file and place it in src-tauri/gen/android."
        );
        Ok(None)
    }
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> std::io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn modify_file(path: PathBuf, regex: Regex, replace: String) {
    let contents = fs::read_to_string(path.clone()).expect("Couldn't find file");
    let new = regex.replace_all(contents.as_str(), replace.as_str());
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .expect("Failed to open file");
    file.write(new.as_bytes()).expect("Failed to write to file");
}

#[cfg(feature = "push-notifications-fcm")]
fn modify_android_sources() {
    let android_library = std::env::var("WRY_ANDROID_LIBRARY")
        .expect("Expected WRY_ANDROID_LIBRARY to be set when targeting android.");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("No OUT_DIR variable found"));

    copy_dir_all(PathBuf::from("android"), out_dir.join("android"))
        .expect("Failed to copy over the android folder");

    modify_file(
        out_dir.join("android/src/main/java/PushNotificationsService.kt"),
        Regex::new(r#"loadLibrary\(".*?"\)"#).unwrap(),
        format!("loadLibrary(\"{android_library}\")"),
    );

    if let Some(google_services_file) = google_services_path().unwrap() {
        let google_services = std::fs::read_to_string(google_services_file)
            .expect("Failed to read google-services.json");
        let json: Value = serde_json::from_str(google_services.as_str())
            .expect("google-services.json file does not contain a JSON object.");
        let Value::String(project_id) = json["project_info"]["project_id"].clone() else {
            panic!("The project_info.project_id property in the google-services.json file is not a string.");
        };
        let Value::String(app_id) = json["client"][0]["client_info"]["mobilesdk_app_id"].clone()
        else {
            panic!("The client[0].client_info.mobilesdk_app_id property in the google-services.json file is not a string.");
        };
        let Value::String(api_key) = json["client"][0]["api_key"][0]["current_key"].clone() else {
            panic!("The client[0].api_key[0].current_key property in the google-services.json file is not a string.");
        };

        modify_file(
            out_dir.join("android/src/main/java/NotificationPlugin.kt"),
            Regex::new(r#"var API_KEY = ".*?""#).unwrap(),
            format!(r#"var API_KEY = "{}""#, api_key),
        );
        modify_file(
            out_dir.join("android/src/main/java/NotificationPlugin.kt"),
            Regex::new(r#"var PROJECT_ID = ".*?""#).unwrap(),
            format!(r#"var PROJECT_ID = "{}""#, project_id),
        );
        modify_file(
            out_dir.join("android/src/main/java/NotificationPlugin.kt"),
            Regex::new(r#"var APP_ID = ".*?""#).unwrap(),
            format!(r#"var APP_ID = "{}""#, app_id),
        );
    }
}

fn main() {
    let mut android_path = String::from("android");

    #[cfg(feature = "push-notifications-fcm")]
    {
        let is_targeting_android = std::env::var("TARGET").unwrap().contains("android");
        if is_targeting_android {
            modify_android_sources();
            android_path = format!(
                "{}/android",
                std::env::var("OUT_DIR").expect("No OUT_DIR variable found")
            );
        }
    }

    let result = tauri_plugin::Builder::new(COMMANDS)
        .global_api_script_path("./api-iife.js")
        .android_path(android_path)
        .ios_path("ios")
        .try_build();

    // when building documentation for Android the plugin build result is always Err() and is irrelevant to the crate documentation build
    if !(cfg!(docsrs) && std::env::var("TARGET").unwrap().contains("android")) {
        result.unwrap();
    }
}
