// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

package app.tauri.notification

import app.tauri.annotation.InvokeArg

/// Per-conversation data needed to render the notification as part of a
/// messaging thread (Android `MessagingStyle` / iOS Communication
/// Notifications). When the parent [Notification.conversationStyle] is
/// non-null, [TauriNotificationManager] renders the notification with
/// `NotificationCompat.MessagingStyle`.
@InvokeArg
class ConversationStyle {
  /// Stable identifier for the message sender (e.g. an agent/user id),
  /// distinct from the sender's display name. Fed into
  /// `Person.Builder.setKey` so successive messages from the same sender —
  /// even within the same group thread — accumulate under one person
  /// identity. Falls back to the notification's [Notification.route] when
  /// unset.
  var senderId: String? = null
}
