// swift-tools-version:5.5
// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT



import PackageDescription

let package = Package(
    name: "tauri-plugin-notification",
    platforms: [
		.iOS(.v11),
    ],
    products: [
        // Products define the executables and libraries a package produces, and make them visible to other packages.
        .library(
            name: "tauri-plugin-notification",
            type: .static,
            targets: ["tauri-plugin-notification"]),
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api"),
        .package(name: "Firebase", url: "https://github.com/firebase/firebase-ios-sdk.git", from: "10.19.0"),
    ],
    targets: [
        // Targets are the basic building blocks of a package. A target can define a module or a test suite.
        // Targets can depend on other targets in this package, and on products in packages this package depends on.
        .target(
            name: "tauri-plugin-notification",
            dependencies: [
                .byName(name: "Tauri"),
                .product(name: "FirebaseMessaging", package: "Firebase"),
            ],
            path: "Sources")
    ]
)
