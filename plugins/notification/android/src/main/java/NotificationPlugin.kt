// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

package app.tauri.notification

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.util.Log
import android.webkit.WebView
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import app.tauri.Logger
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.LifecycleOwner
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.installations.FirebaseInstallations
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

const val LOCAL_NOTIFICATIONS = "permissionState"

@InvokeArg
class PluginConfig {
  var icon: String? = null
  var sound: String? = null
  var iconColor: String? = null
}

@InvokeArg
class BatchArgs {
  lateinit var notifications: List<Notification>
}

@InvokeArg
class CancelArgs {
  lateinit var notifications: List<Int>
}

@InvokeArg
class NotificationAction {
  lateinit var id: String
  var title: String? = null
  var input: Boolean? = null
}

@InvokeArg
class ActionType {
  lateinit var id: String
  lateinit var actions: List<NotificationAction>
}

@InvokeArg
class RegisterActionTypesArgs {
  lateinit var types: List<ActionType>
}

@InvokeArg
class ActiveNotification {
  var id: Int = 0
  var tag: String? = null
}

@InvokeArg
class RemoveActiveArgs {
  var notifications: List<ActiveNotification> = listOf()
}

@TauriPlugin(
  permissions = [
    Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "permissionState")
  ]
)
class NotificationPlugin(private val activity: Activity): Plugin(activity) {
  private var webView: WebView? = null
  private lateinit var manager: TauriNotificationManager
  private lateinit var notificationManager: NotificationManager
  private lateinit var notificationStorage: NotificationStorage
  private var channelManager = ChannelManager(activity)
  private var fcmToken: String? = null
  /// Route requested by a tap before the webview was ready (app launched by
  /// tapping a notification from terminated state). Applied once the webview
  /// is attached and its URL has been set.
  private var pendingRoute: String? = null
  /// Tracks whether the activity is currently in the foreground. Mirrors the
  /// iOS `willPresent` semantics: route-suppression only applies while the
  /// app is foregrounded; backgrounded notifications are always shown.
  ///
  /// Driven by a LifecycleEventObserver attached to ProcessLifecycleOwner in
  /// `load()` (rather than overriding the plugin's own `onResume/onPause`),
  /// because Tauri may load this plugin *after* the activity's first
  /// `onResume()` has already iterated its plugins — in which case the
  /// override would never fire for the initial transition. The observer fires
  /// synchronously on registration with the current state, so we get the
  /// correct value regardless of load order.
  @Volatile private var isForeground: Boolean = false

  companion object {
    var instance: NotificationPlugin? = null

    fun triggerNotification(notification: Notification) {
      instance?.triggerObject("notification", notification)
    }
  }

  /// Returns true iff the activity is foregrounded and the webview's current
  /// path equals `route`. Mirrors the iOS `willPresent` foreground-suppression
  /// check. Called from the FCM service thread (`PushNotificationsService`),
  /// so the JS evaluation is dispatched to the UI thread and awaited briefly.
  fun isViewingRoute(route: String): Boolean {
    Log.i("NotificationPlugin", "isViewingRoute(\"$route\"): isForeground=$isForeground, webView=${webView != null}")
    if (!isForeground) {
      Log.i("NotificationPlugin", "isViewingRoute: bail — not foreground")
      return false
    }
    val view = webView
    if (view == null) {
      Log.i("NotificationPlugin", "isViewingRoute: bail — webView is null")
      return false
    }

    val latch = CountDownLatch(1)
    var currentPath: String? = null
    activity.runOnUiThread {
      view.evaluateJavascript("window.location.pathname") { jsResult ->
        // evaluateJavascript returns a JSON-encoded value, e.g. "\"/foo\"".
        currentPath = jsResult?.removeSurrounding("\"")
        Log.i("NotificationPlugin", "isViewingRoute: evaluateJavascript returned raw=$jsResult, parsed=$currentPath")
        latch.countDown()
      }
    }
    return try {
      val signaled = latch.await(500, TimeUnit.MILLISECONDS)
      val matches = currentPath == route
      Log.i("NotificationPlugin", "isViewingRoute: signaled=$signaled, currentPath=$currentPath, route=$route, matches=$matches")
      if (signaled) matches else false
    } catch (_: InterruptedException) {
      Log.i("NotificationPlugin", "isViewingRoute: interrupted")
      false
    }
  }

  override fun load(webView: WebView) {
    instance = this
    Log.i("NotificationPlugin", "load: instance set, webView attached")

    // Observe the activity's lifecycle so we know whether it's foregrounded.
    // Done here (rather than via the plugin's onResume/onPause overrides)
    // because Tauri may load this plugin after the activity's first onResume
    // has already iterated its plugin set — in which case those overrides
    // never fire for the initial transition. addObserver dispatches all
    // pending events to bring the observer to the registry's current state,
    // so we get the right value even when registering post-resume.
    // Lifecycle registration must run on the main thread.
    activity.runOnUiThread {
      (activity as LifecycleOwner).lifecycle.addObserver(LifecycleEventObserver { _, event ->
        when (event) {
          Lifecycle.Event.ON_RESUME -> {
            isForeground = true
            Log.i("NotificationPlugin", "lifecycle ON_RESUME: isForeground=true")
          }
          Lifecycle.Event.ON_PAUSE -> {
            isForeground = false
            Log.i("NotificationPlugin", "lifecycle ON_PAUSE: isForeground=false")
          }
          else -> {}
        }
      })
    }

    super.load(webView)
    this.webView = webView
    // If a tap arrived before the webview was attached, apply it now that the
    // webview is available.
    applyPendingRouteIfNeeded()
    notificationStorage = NotificationStorage(activity, jsonMapper())
    
    val manager = TauriNotificationManager(
      notificationStorage,
      activity,
      activity,
      getConfig(PluginConfig::class.java)
    )
    manager.createNotificationChannel()
    this.manager = manager
    
    notificationManager = activity.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    // This may be replaced at compile time by build.rs
    var API_KEY = "<API_KEY>"
    var PROJECT_ID = "<PROJECT_ID>"
    var APP_ID = "<APP_ID>"

    if (API_KEY != "<API_KEY>") {
      val options = FirebaseOptions.Builder().setApiKey(API_KEY)
          .setProjectId(PROJECT_ID)
          .setApplicationId(APP_ID).build()

      FirebaseApp.initializeApp(activity, options)
    }

    val intent = activity.intent
    intent?.let {
      onIntent(it)
    }
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    onIntent(intent)
  }

  fun onIntent(intent: Intent) {
    if (Intent.ACTION_MAIN != intent.action) {
      return
    }
    val dataJson = manager.handleNotificationActionPerformed(intent, notificationStorage)
    if (dataJson != null) {
      trigger("actionPerformed", dataJson)

      // Navigate the webview to the route this notification carries (taps only —
      // dismiss shouldn't navigate).
      val actionId = dataJson.getString("actionId", null)
      if (actionId == "tap") {
        val notification = dataJson.getJSObject("notification")
        val route = notification?.getString("route", null)
        if (!route.isNullOrEmpty()) {
          navigateWebView(route)
        }
      }
    }
  }

  private fun navigateWebView(route: String) {
    pendingRoute = route
    applyPendingRouteIfNeeded()
  }

  /// Apply a pending tap-route to the webview if both are ready. If the
  /// webview is attached but its URL is still null (the initial page hasn't
  /// begun loading yet — typical when the app was launched by tapping a
  /// notification from terminated state), retry on a short delay until it
  /// becomes available.
  private fun applyPendingRouteIfNeeded(remainingRetries: Int = 50) {
    val route = pendingRoute ?: return
    val view = webView ?: return
    activity.runOnUiThread {
      val current = view.url
      if (current.isNullOrEmpty()) {
        if (remainingRetries > 0) {
          view.postDelayed({ applyPendingRouteIfNeeded(remainingRetries - 1) }, 100)
        }
        return@runOnUiThread
      }
      val newUrl = try {
        android.net.Uri.parse(current).buildUpon().path(route).build().toString()
      } catch (_: Exception) {
        pendingRoute = null
        return@runOnUiThread
      }
      view.loadUrl(newUrl)
      pendingRoute = null
    }
  }

  @Command
  fun registerForPushNotifications(invoke: Invoke) {
    FirebaseMessaging.getInstance().getToken().addOnCompleteListener { task ->
      if (!task.isSuccessful) {
          Logger.error(Logger.tags("Notification"), "Fetching FCM registration token failed", task.exception)
          invoke.reject("Fetching FCM registration token failed", task.exception)
          return@addOnCompleteListener
      }

      fcmToken = task.result
      Logger.info("Registered for FCM with token:", task.result)
      val data = JSObject()
      data.put("token", fcmToken)
      invoke.resolve(data)
    }
  }

  @Command
  fun fcmProjectId(invoke: Invoke) {
    var PROJECT_ID = "<PROJECT_ID>"
    val data = JSObject()
    data.put("fcmProjectId", PROJECT_ID)
    invoke.resolve(data)
  }
  
  @Command
  fun show(invoke: Invoke) {
    val notification = invoke.parseArgs(Notification::class.java)
    notification.sourceJson = jsonMapper().writeValueAsString(notification)
    val id = manager.schedule(notification)

    invoke.resolveObject(id)
  }

  @Command
  fun batch(invoke: Invoke) {
    val args = invoke.parseArgs(BatchArgs::class.java)

    val ids = manager.schedule(args.notifications)
    notificationStorage.appendNotifications(args.notifications)

    invoke.resolveObject(ids)
  }

  @Command
  fun cancel(invoke: Invoke) {
    val args = invoke.parseArgs(CancelArgs::class.java)
    manager.cancel(args.notifications)
    invoke.resolve()
  }

  @Command
  fun removeActive(invoke: Invoke) {
    val args = invoke.parseArgs(RemoveActiveArgs::class.java)

    if (args.notifications.isEmpty()) {
      notificationManager.cancelAll()
      invoke.resolve()
    } else {
      for (notification in args.notifications) {
        if (notification.tag == null) {
          notificationManager.cancel(notification.id)
        } else {
          notificationManager.cancel(notification.tag, notification.id)
        }
      }
      invoke.resolve()
    }
  }

  @Command
  fun getPending(invoke: Invoke) {
    val notifications= notificationStorage.getSavedNotifications()
    val result = Notification.buildNotificationPendingList(notifications)
    invoke.resolveObject(result)
  }

  @Command
  fun registerActionTypes(invoke: Invoke) {
    val args = invoke.parseArgs(RegisterActionTypesArgs::class.java)
    notificationStorage.writeActionGroup(args.types)
    invoke.resolve()
  }

  @SuppressLint("ObsoleteSdkInt")
  @Command
  fun getActive(invoke: Invoke) {
    val notifications = JSArray()
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
      val activeNotifications = notificationManager.activeNotifications
      for (activeNotification in activeNotifications) {
        val jsNotification = JSObject()
        jsNotification.put("id", activeNotification.id)
        jsNotification.put("tag", activeNotification.tag)
        val notification = activeNotification.notification
        if (notification != null) {
          jsNotification.put("title", notification.extras.getCharSequence(android.app.Notification.EXTRA_TITLE))
          jsNotification.put("body", notification.extras.getCharSequence(android.app.Notification.EXTRA_TEXT))
          jsNotification.put("group", notification.group)
          jsNotification.put(
            "groupSummary",
            0 != notification.flags and android.app.Notification.FLAG_GROUP_SUMMARY
          )
          val extras = JSObject()
          for (key in notification.extras.keySet()) {
            extras.put(key!!, notification.extras.getString(key))
          }
          jsNotification.put("data", extras)
        }
        notifications.put(jsNotification)
      }
    }
    
    invoke.resolveObject(notifications)
  }

  @Command
  fun createChannel(invoke: Invoke) {
    channelManager.createChannel(invoke)
  }

  @Command
  fun deleteChannel(invoke: Invoke) {
    channelManager.deleteChannel(invoke)
  }

  @Command
  fun listChannels(invoke: Invoke) {
    channelManager.listChannels(invoke)
  }

  @Command
  override fun checkPermissions(invoke: Invoke) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
      val permissionsResultJSON = JSObject()
      permissionsResultJSON.put("permissionState", getPermissionState())
      invoke.resolve(permissionsResultJSON)
    } else {
      super.checkPermissions(invoke)
    }
  }

  @Command
  override fun requestPermissions(invoke: Invoke) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
      permissionState(invoke)
    } else {
      if (getPermissionState(LOCAL_NOTIFICATIONS) !== PermissionState.GRANTED) {
        requestPermissionForAlias(LOCAL_NOTIFICATIONS, invoke, "permissionsCallback")
      }
    }
  }

  @Command
  fun permissionState(invoke: Invoke) {
    val permissionsResultJSON = JSObject()
    permissionsResultJSON.put("permissionState", getPermissionState())
    invoke.resolve(permissionsResultJSON)
  }

  @PermissionCallback
  private fun permissionsCallback(invoke: Invoke) {
    val permissionsResultJSON = JSObject()
    permissionsResultJSON.put("permissionState", getPermissionState())
    invoke.resolve(permissionsResultJSON)
  }

  private fun getPermissionState(): String {
    return if (manager.areNotificationsEnabled()) {
      "granted"
    } else {
      "denied"
    }
  }
}
