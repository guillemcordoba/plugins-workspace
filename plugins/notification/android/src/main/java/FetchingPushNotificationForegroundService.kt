package app.tauri.notification

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import com.fasterxml.jackson.module.kotlin.jsonMapper
import java.util.concurrent.TimeUnit

/**
 * Keeps the process in the foreground while a high-priority push is turned
 * into a notification, after Signal's FcmFetchForegroundService. OEM freezers
 * (Transsion's Hiber) freeze a background process seconds after FCM wakes it,
 * force-releasing FCM's wakelock, so the notification only shows when
 * something thaws the app again.
 */
class FetchingPushNotificationForegroundService : Service() {
  private var wakeLock: PowerManager.WakeLock? = null

  companion object {
    private const val TAG = "FetchingPushNotification"
    private const val CHANNEL_ID = "fetching_push_notification"
    private const val NOTIFICATION_ID = 0x5044
    private const val KEY_STOP_SELF = "stop_self"
    private const val WAKELOCK_TAG = "dashchat:FetchingPushNotification"
    /** Past the push handler's own budget, so a handler that overruns it
     *  releases the lock before the lock gives up. */
    private val WAKELOCK_TIMEOUT_MS = TimeUnit.MINUTES.toMillis(1)
    private const val DEFAULT_TITLE = "Checking for messages…"

    init {
      System.loadLibrary("tauri_app_lib")
    }

    enum class State {
      STOPPED,
      STARTED,
      STOPPING,
      RESTARTING
    }

    private var state = State.STOPPED

    /** Start the service unless it is already running. False when Android
     *  refused the start. */
    fun startIfNecessary(context: Context): Boolean {
      synchronized(this) {
        when (state) {
          State.STOPPING -> state = State.RESTARTING
          State.STOPPED -> {
            state = try {
              ContextCompat.startForegroundService(
                context,
                Intent(context, FetchingPushNotificationForegroundService::class.java)
              )
              State.STARTED
            } catch (e: IllegalStateException) {
              Log.e(TAG, "Failed to start the foreground service", e)
              State.STOPPED
              return false
            }
          }
          else -> Log.i(TAG, "Already started the foreground service")
        }
      }
      return true
    }

    /** Stops through a start intent: a foreground service that is stopped
     *  before its onStartCommand ran still has to call startForeground, or
     *  Android crashes the app. */
    fun stopIfNecessary(context: Context) {
      synchronized(this) {
        when (state) {
          State.STARTED -> {
            state = State.STOPPING
            try {
              context.startService(
                Intent(context, FetchingPushNotificationForegroundService::class.java)
                  .putExtra(KEY_STOP_SELF, true)
              )
            } catch (e: IllegalStateException) {
              Log.w(TAG, "Failed to stop the foreground service, assuming it is stopped", e)
              state = State.STOPPED
            }
          }
          State.RESTARTING -> state = State.STOPPED
          else -> Log.i(TAG, "No foreground service to stop")
        }
      }
    }

    private fun onServiceDestroyed(context: Context) {
      synchronized(this) {
        when (state) {
          State.RESTARTING -> {
            state = State.STOPPED
            Log.i(TAG, "Restarting the foreground service")
            startIfNecessary(context)
          }
          else -> state = State.STOPPED
        }
      }
    }
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    try {
      postForegroundNotification()
    } catch (e: RuntimeException) {
      Log.w(TAG, "Failed to post the foreground notification", e)
    }
    return if (intent?.getBooleanExtra(KEY_STOP_SELF, false) == true) {
      releaseWakeLock()
      stopForeground(STOP_FOREGROUND_REMOVE)
      stopSelf()
      START_NOT_STICKY
    } else {
      if (wakeLock == null) {
        val powerManager = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, WAKELOCK_TAG).apply {
          acquire(WAKELOCK_TIMEOUT_MS)
        }
      }
      START_STICKY
    }
  }

  override fun onDestroy() {
    releaseWakeLock()
    onServiceDestroyed(this)
  }

  private fun releaseWakeLock() {
    val lock = wakeLock ?: return
    if (lock.isHeld) lock.release()
    wakeLock = null
  }

  private fun postForegroundNotification() {
    val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      manager.createNotificationChannel(
        NotificationChannel(CHANNEL_ID, DEFAULT_TITLE, NotificationManager.IMPORTANCE_MIN)
      )
    }
    val notification = NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(applicationInfo.icon)
      .setContentTitle(title())
      .setCategory(NotificationCompat.CATEGORY_SERVICE)
      .setProgress(0, 0, true)
      .setSilent(true)
      .build()
    if (Build.VERSION.SDK_INT >= 34) {
      startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_REMOTE_MESSAGING)
    } else {
      startForeground(NOTIFICATION_ID, notification)
    }
  }

  /** The title the app localizes, or the English one when the app gives none. */
  private fun title(): String {
    val title = try {
      val json = fetchingpushnotificationdata(applicationInfo.dataDir)
      jsonMapper().readValue(json, Notification::class.java).title
    } catch (e: Throwable) {
      Log.w(TAG, "The app gave no title for the foreground notification", e)
      null
    }
    return if (title.isNullOrBlank()) DEFAULT_TITLE else title
  }

  private external fun fetchingpushnotificationdata(dataDir: String): String

  override fun onBind(intent: Intent?): IBinder? = null
}
