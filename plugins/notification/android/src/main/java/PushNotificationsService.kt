package app.tauri.notification

import android.util.Log
import android.content.Context
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import app.tauri.plugin.JSObject
import app.tauri.plugin.Channel
import com.fasterxml.jackson.module.kotlin.jsonMapper
import java.util.concurrent.Executors

class PushNotificationsService(): FirebaseMessagingService()  {

    companion object {
        init {
            System.loadLibrary("tauri_app_lib")
        }
        var contextInitialized: Boolean = false

        /** One push at a time, off the FCM thread: FCM hands over the next
         *  message only once onMessageReceived returns, and may destroy this
         *  service right after. After Signal's FcmFetchManager. */
        private val EXECUTOR = Executors.newSingleThreadExecutor()
        private var activeCount = 0
    }

    /**
     * Called if InstanceID token is updated. This may occur if the security of
     * the previous token had been compromised. Note that this is called when the InstanceID token
     * is initially generated so this is where you would retrieve the token.
     */
    override fun onNewToken(token: String) {
        super.onNewToken(token)
        Log.i("PushNotificationsService ", "Refreshed token :: $token")
        // If you want to send messages to this application instance or
        // manage this apps subscriptions on the server side, send the
        // Instance ID token to your app server.
        val data = JSObject()
        data.put("token", token)
        NotificationPlugin.instance?.trigger("newFcmToken", data)
    }

    override fun onMessageReceived(message: RemoteMessage) {
        super.onMessageReceived(message)
        Log.i("PushNotificationService ", "Message :: $message, priority ${message.priority}")

        // Only a high-priority message lets a background app start a
        // foreground service.
        val context = applicationContext
        if (message.priority == RemoteMessage.PRIORITY_HIGH) {
            FetchingPushNotificationForegroundService.startIfNecessary(context)
        }
        synchronized(PushNotificationsService) { activeCount++ }
        EXECUTOR.execute {
            try {
                showNotificationFor(context, message)
            } finally {
                synchronized(PushNotificationsService) {
                    activeCount--
                    if (activeCount <= 0) {
                        FetchingPushNotificationForegroundService.stopIfNecessary(context)
                    }
                }
            }
        }
    }

    private fun showNotificationFor(context: Context, message: RemoteMessage) {

        val data = JSObject()

        for (entry in message.data.entries.iterator()) {
            data.put(entry.key, entry.value)
        }

        val notificationStorage = NotificationStorage(context, jsonMapper())
        val manager = TauriNotificationManager(
          notificationStorage,
          context,
          null
        )

        val d = data.toString()
        val dataDir = context.getApplicationInfo().dataDir 

        val notification = receivepushnotification(d, dataDir)
        Log.i("PushNotificationService ", "data:: $d")
        Log.i("PushNotificationService ", "Notifications :: $notification")
        val modifiedNotification = jsonMapper().readValue(notification, Notification::class.java)

        if (modifiedNotification.title == null && modifiedNotification.body == null) {
            Log.i("PushNotificationService", "Skipping notification: title and body both null")
            return
        }

        val scheduleNotification = {
            modifiedNotification.sourceJson = notification
            manager.schedule(modifiedNotification)
        }

        // Mirror iOS `willPresent`: if the user is already foregrounded on the
        // route this notification points at, don't show a banner.
        val route = modifiedNotification.route
        val pluginInstance = NotificationPlugin.instance
        if (!route.isNullOrEmpty() && pluginInstance != null) {
            pluginInstance.isViewingRoute(route) { isViewing ->
                if (isViewing) {
                    Log.i("PushNotificationService", "Suppressing notification: user is viewing $route")
                } else {
                    Log.i("PushNotificationService", "Showing notification (no suppression)")
                    scheduleNotification()
                }
            }
        } else {
            Log.i("PushNotificationService", "Showing notification (no route or no plugin instance)")
            scheduleNotification()
        }
    }

    private external fun receivepushnotification(notification: String, dataDir: String): String
}
