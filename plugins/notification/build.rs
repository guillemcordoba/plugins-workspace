// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use regex::Regex;
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

fn main() {
    let is_targeting_android = std::env::var("TARGET").unwrap().contains("android");
    if is_targeting_android {
        let android_library = std::env::var("WRY_ANDROID_LIBRARY")
            .expect("Expected WRY_ANDROID_LIBRARY to be set when targeting android.");

        let push_notifications_service_path = "android/src/main/java/PushNotificationsService.kt";
        let re = Regex::new(r#"loadLibrary\(".*?"\)"#).unwrap();

        let contents = fs::read_to_string(push_notifications_service_path)
            .expect("Couldn't find PushNotificationsService");
        let new = re.replace(
            contents.as_str(),
            format!("loadLibrary(\"{android_library}\")").as_str(),
        );
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(push_notifications_service_path)
            .expect("Failed to open PushNotificationsService");
        file.write(new.as_bytes())
            .expect("Failed to write to PushNotificationsService");
    }

    if let Err(error) = tauri_build::mobile::PluginBuilder::new()
        .android_path("android")
        .ios_path("ios")
        .run()
    {
        println!("{error:#}");
        // when building documentation for Android the plugin build result is irrelevant to the crate itself
        if !(cfg!(docsrs) && is_targeting_android) {
            std::process::exit(1);
        }
    }
}
