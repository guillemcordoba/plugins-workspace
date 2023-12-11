// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use std::{process::Command, path::Path};

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os == "ios" {
        // Determine Xcode directory path
        let xcode_select_output = Command::new("xcode-select").arg("-p").output().unwrap();
        if !xcode_select_output.status.success() {
            panic!("Failed to run xcode-select -p");
        }
        let xcode_dir = String::from_utf8(xcode_select_output.stdout)
            .unwrap()
            .trim()
            .to_string();

        // Determine SDK directory paths
        let sdk_dir_ios = Path::new(&xcode_dir)
            .join("Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk")
            .to_str()
            .unwrap()
            .to_string();
        //let ios_simulator_version_min = "9.0.0";
        let ios_version_min = "9.0.0";
        println!("AAAA{sdk_dir_ios}");

        std::env::set_var("SDKROOT", sdk_dir_ios.clone());
        std::env::set_var("CFLAGS", format!(" -isysroot {sdk_dir_ios}  -mios-version-min={ios_version_min}"));
        std::env::set_var("CGO_LDFLAGS", format!("-s -w -extldflags \"-lresolv\""));
        std::env::set_var("LDFLAGS", format!("-s -w -extldflags \"-lresolv\""));
    }

    if let Err(error) = tauri_build::mobile::PluginBuilder::new()
        .android_path("android")
        .ios_path("ios")
        .run()
    {
        println!("{error:#}");
        // when building documentation for Android the plugin build result is irrelevant to the crate itself
        if !(cfg!(docsrs) && std::env::var("TARGET").unwrap().contains("android")) {
            std::process::exit(1);
        }
    }
}
