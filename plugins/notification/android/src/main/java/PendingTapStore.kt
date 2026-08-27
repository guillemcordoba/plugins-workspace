// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

package app.tauri.notification

import android.content.Context
import android.content.Intent
import androidx.core.app.RemoteInput
import org.json.JSONObject

/// One notification tap captured by [NotificationTapActivity], pending
/// consumption by [NotificationPlugin] once the plugin is loaded.
class PendingTap(
  val notificationId: Int,
  val actionId: String?,
  val inputValue: String?,
  val isRemovable: Boolean,
  val sourceJson: String?,
  val at: Long,
)

/// Persists the last notification tap across the gap between the tap itself
/// (handled by [NotificationTapActivity], possibly before the Tauri runtime
/// exists) and the moment [NotificationPlugin] is loaded and can act on it.
/// SharedPreferences-backed so a tap that cold-starts the app survives until
/// `load()`.
class PendingTapStore(context: Context) {
  private val prefs =
    context.getSharedPreferences("tauri_notification_pending_tap", Context.MODE_PRIVATE)

  fun save(intent: Intent) {
    val id = intent.getIntExtra(NOTIFICATION_INTENT_KEY, Int.MIN_VALUE)
    if (id == Int.MIN_VALUE) return
    val obj = JSONObject()
    obj.put("notificationId", id)
    obj.put("actionId", intent.getStringExtra(ACTION_INTENT_KEY))
    obj.put("isRemovable", intent.getBooleanExtra(NOTIFICATION_IS_REMOVABLE_KEY, true))
    obj.put("sourceJson", intent.getStringExtra(NOTIFICATION_OBJ_INTENT_KEY))
    RemoteInput.getResultsFromIntent(intent)?.getCharSequence(REMOTE_INPUT_KEY)?.let {
      obj.put("inputValue", it.toString())
    }
    obj.put("at", System.currentTimeMillis())
    synchronized(lock) {
      // commit() (not apply()): the trampoline activity finishes immediately
      // and the process may be torn down before an async write lands.
      prefs.edit().putString(KEY, obj.toString()).commit()
    }
  }

  /// Returns the stored tap and removes it, or null when there is none or it
  /// is older than [maxAgeMs] (a tap that never got consumed — e.g. a crash
  /// right after the trampoline persisted it — must not replay a navigation
  /// arbitrarily later).
  fun consume(maxAgeMs: Long): PendingTap? {
    val raw = synchronized(lock) {
      val stored = prefs.getString(KEY, null) ?: return null
      prefs.edit().remove(KEY).commit()
      stored
    }
    return try {
      val obj = JSONObject(raw)
      val tap = PendingTap(
        notificationId = obj.getInt("notificationId"),
        actionId = obj.stringOrNull("actionId"),
        inputValue = obj.stringOrNull("inputValue"),
        isRemovable = obj.optBoolean("isRemovable", true),
        sourceJson = obj.stringOrNull("sourceJson"),
        at = obj.getLong("at"),
      )
      if (System.currentTimeMillis() - tap.at > maxAgeMs) null else tap
    } catch (_: Exception) {
      null
    }
  }

  private fun JSONObject.stringOrNull(name: String): String? =
    if (has(name) && !isNull(name)) getString(name) else null

  companion object {
    private const val KEY = "pending_tap"
    private val lock = Any()
  }
}
