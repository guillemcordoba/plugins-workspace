package app.tauri.notification

import android.util.Log
import android.content.Context
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import app.tauri.plugin.JSObject
import app.tauri.plugin.Channel
import com.fasterxml.jackson.module.kotlin.jsonMapper

class PushNotificationsService(): FirebaseMessagingService()  {

    companion object {
        init {
            System.loadLibrary("tauri_app_lib")
        }
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
        Log.i("PushNotificationService ", "Message :: $message")

        val data = JSObject()

        for (entry in message.data.entries.iterator()) {
            data.put(entry.key, entry.value)
        }

        val notificationStorage = NotificationStorage(this, jsonMapper())
        val manager = TauriNotificationManager(
          notificationStorage,
		  null,
          this,
          null
        )

        val d = data.toString()

        Log.i("yo", "tes")
        NotificationPlugin.instance?.let {
            Log.i("isnuuuul", "tes")
            Log.i("PushNotificationService ", "data:: $d")
            val notification = receivepushnotification(d)
            Log.i("PushNotificationService ", "Notifications :: $notification")
            val modifiedNotification = jsonMapper().readValue(notification, Notification::class.java)

            if (!(modifiedNotification.title == null && modifiedNotification.body == null)) {
                modifiedNotification.sourceJson = notification
                manager.schedule(modifiedNotification)
            }
        } ?: run {
            Log.i("PushNotificationService ", "data:: $d")
            val notification = initcontextandreceivepushnotification(this, d)
            Log.i("PushNotificationService ", "Notifications :: $notification")
            val modifiedNotification = jsonMapper().readValue(notification, Notification::class.java)

            if (!(modifiedNotification.title == null && modifiedNotification.body == null)) {
                modifiedNotification.sourceJson = notification
                manager.schedule(modifiedNotification)
            }
        }
    }

    private external fun initcontextandreceivepushnotification(context: Context, notification: String): String

    private external fun receivepushnotification(notification: String): String
}
