// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

package app.tauri.notification

import android.app.Activity
import android.os.Bundle

/// Invisible trampoline target for notification tap/action PendingIntents.
///
/// A tap intent delivered straight to the main activity is unreliable: on a
/// cold start into an existing task it reaches `PluginManager.onNewIntent`
/// before this plugin is registered and is silently lost, while
/// `Activity.getIntent()` keeps returning whatever intent the task was
/// originally created with — replaying a stale tap on every cold start.
/// Routing taps through this activity sidesteps both: the payload is persisted
/// here (plugin code that always runs at tap time), and the main activity is
/// launched with a plain intent carrying no tap state at all. The plugin
/// consumes the persisted tap from `load()` / `ON_RESUME`.
///
/// Activity trampolines are exempt from the Android 12 notification-trampoline
/// restriction, which only covers receivers and services.
class NotificationTapActivity : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    PendingTapStore(this).save(intent)
    packageManager.getLaunchIntentForPackage(packageName)?.let { startActivity(it) }
    // Theme.NoDisplay requires finishing before resume.
    finish()
  }
}
