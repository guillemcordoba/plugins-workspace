package app.tauri.notification

import android.util.Log
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import app.tauri.plugin.JSObject
import app.tauri.plugin.Channel
import com.fasterxml.jackson.module.kotlin.jacksonObjectMapper
import com.fasterxml.jackson.module.kotlin.jsonMapper
import com.fasterxml.jackson.module.kotlin.readValue 

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
        // manager.createNotificationChannel()

        val d = data.toString()

        Log.i("PushNotificationService ", "data:: $d")
        val notifications = modifypushnotification(data.toString())
        Log.i("PushNotificationService ", "Notifications :: $notifications")

        val mapper = jacksonObjectMapper()

        // val type = jsonMapper().getTypeFactory().constructCollectionType(List::class.java, Notification::class.java)
        val modifiedNotifications: List<Notification> = mapper.readValue(notifications, Array<Notification>::class.java).toList()
        for (notification in modifiedNotifications) {
            notification.sourceJson = jsonMapper().writeValueAsString(notification)
            manager.schedule(notification)
        }
    }

    private external fun modifypushnotification(notification: String): String
}
