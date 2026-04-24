// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

import SwiftRs
import Tauri
import UIKit
import UserNotifications
import WebKit
import FirebaseMessaging
import FirebaseCore

enum ShowNotificationError: LocalizedError {
  case make(Error)
  case create(Error)

  var errorDescription: String? {
    switch self {
    case .make(let error):
      return "Unable to make notification: \(error)"
    case .create(let error):
      return "Unable to create notification: \(error)"
    }
  }
}

enum ScheduleEveryKind: String, Decodable {
  case year
  case month
  case twoWeeks
  case week
  case day
  case hour
  case minute
  case second
}

struct ScheduleInterval: Decodable {
  var year: Int?
  var month: Int?
  var day: Int?
  var weekday: Int?
  var hour: Int?
  var minute: Int?
  var second: Int?
}

enum NotificationSchedule: Decodable {
  case at(date: String, repeating: Bool)
  case interval(interval: ScheduleInterval)
  case every(interval: ScheduleEveryKind, count: Int)
}

struct NotificationAttachmentOptions: Codable {
  let iosUNNotificationAttachmentOptionsTypeHintKey: String?
  let iosUNNotificationAttachmentOptionsThumbnailHiddenKey: String?
  let iosUNNotificationAttachmentOptionsThumbnailClippingRectKey: String?
  let iosUNNotificationAttachmentOptionsThumbnailTimeKey: String?
}

struct NotificationAttachment: Codable {
  let id: String
  let url: String
  let options: NotificationAttachmentOptions?
}

struct Notification: Decodable {
  let id: Int
  var title: String
  var body: String?
  var extra: [String: String]?
  var schedule: NotificationSchedule?
  var attachments: [NotificationAttachment]?
  var sound: String?
  var group: String?
  var actionTypeId: String?
  var summary: String?
  var silent: Bool?
}

struct RemoveActiveNotification: Decodable {
  let id: Int
}

struct RemoveActiveArgs: Decodable {
  let notifications: [RemoveActiveNotification]
}

func showNotification(invoke: Invoke, notification: Notification)
  throws -> UNNotificationRequest
{
  var content: UNNotificationContent
  do {
    content = try makeNotificationContent(notification)
  } catch {
    throw ShowNotificationError.make(error)
  }

  var trigger: UNNotificationTrigger?

  do {
    if let schedule = notification.schedule {
      try trigger = handleScheduledNotification(schedule)
    }
  } catch {
    throw ShowNotificationError.create(error)
  }

  // Schedule the request.
  let request = UNNotificationRequest(
    identifier: "\(notification.id)", content: content, trigger: trigger
  )

  let center = UNUserNotificationCenter.current()
  center.add(request) { (error: Error?) in
    if let theError = error {
      invoke.reject(theError.localizedDescription)
    }
  }

  return request
}

struct CancelArgs: Decodable {
  let notifications: [Int]
}

struct Action: Decodable {
  let id: String
  let title: String
  var requiresAuthentication: Bool?
  var foreground: Bool?
  var destructive: Bool?
  var input: Bool?
  var inputButtonTitle: String?
  var inputPlaceholder: String?
}

struct ActionType: Decodable {
  let id: String
  let actions: [Action]
  var hiddenPreviewsBodyPlaceholder: String?
  var customDismissAction: Bool?
  var allowInCarPlay: Bool?
  var hiddenPreviewsShowTitle: Bool?
  var hiddenPreviewsShowSubtitle: Bool?
  var hiddenBodyPlaceholder: String?
}

struct RegisterActionTypesArgs: Decodable {
  let types: [ActionType]
}

struct BatchArgs: Decodable {
  let notifications: [Notification]
}

class NotificationPlugin: Plugin, MessagingDelegate {
  let notificationHandler = NotificationHandler()
  let notificationManager = NotificationManager()
  var fcmToken: String?
  var registerInvoke: Invoke?

  private static var apnsHookInstalled = false

  override init() {
    super.init()
    notificationManager.notificationHandler = notificationHandler
    notificationHandler.plugin = self
  }

  override public func load(webview: WKWebView) {
    Messaging.messaging().delegate = self

    // Install APNS hook on the live UIApplicationDelegate class as early as
    // possible — before iOS ever delivers a push token. Firebase's own
    // AppDelegate proxy doesn't reliably intercept the callback in Tauri's
    // Rust-backed AppDelegate, so we insert our own forwarder and chain to
    // any previous implementation via a captured IMP.
    installApnsDelegateSwizzle()
  }

  // Called by Firebase whenever a fresh FCM token is available.
  func messaging(_ messaging: Messaging, didReceiveRegistrationToken fcmToken: String?) {
    Logger.info("messaging delegate: FCM token=\(String(describing: fcmToken))")

    self.fcmToken = fcmToken

    guard let token = fcmToken else { return }

    // Always emit the newFcmToken event so JS listeners (whether attached
    // before or after registerForPushNotifications) can observe every token
    // delivery, including the initial one.
    var eventData = JSObject()
    eventData["token"] = token
    try? self.trigger("newFcmToken", data: eventData)

    // Also resolve the pending registerForPushNotifications invoke, if any.
    if let invoke = self.registerInvoke {
      var data = JSObject()
      data["token"] = token
      invoke.resolve(data)
      self.registerInvoke = nil
    }
  }

  @objc public func registerForPushNotifications(_ invoke: Invoke) throws {
    Logger.info("registerForPushNotifications invoked")

    // If Firebase already delivered a token to our MessagingDelegate earlier
    // (either from a prior call in this session or from auto-init on launch
    // after a previous consent), resolve immediately. The delegate only fires
    // on fresh/changed tokens, so otherwise this invoke would wait forever.
    if let existingToken = self.fcmToken {
      Logger.info("registerForPushNotifications: returning cached FCM token")
      var data = JSObject()
      data["token"] = existingToken
      invoke.resolve(data)
      return
    }

    registerInvoke = invoke

    DispatchQueue.main.async {
      UNUserNotificationCenter.current().delegate = self.notificationManager
      let authOptions: UNAuthorizationOptions = [.alert, .badge, .sound]
      UNUserNotificationCenter.current().requestAuthorization(
        options: authOptions,
        completionHandler: { granted, error in
          Logger.info("UN authorization granted=\(granted) error=\(String(describing: error))")
          guard granted else {
            if let invoke = self.registerInvoke {
              invoke.reject(error?.localizedDescription ?? "Notification permission denied")
              self.registerInvoke = nil
            }
            return
          }
          DispatchQueue.main.async {
            // User just consented — enable FCM auto-init so Firebase exchanges
            // the APNS token for an FCM token and our MessagingDelegate fires.
            Messaging.messaging().isAutoInitEnabled = true
            Logger.info("calling UIApplication.registerForRemoteNotifications()")
            UIApplication.shared.registerForRemoteNotifications()

            // Also ask Firebase directly for the current FCM token. If one is
            // already cached (e.g. from a prior session with consent, where
            // the MessagingDelegate hasn't fired again this session), this
            // resolves the invoke without waiting on the delegate.
            Messaging.messaging().token { token, error in
              Logger.info("Messaging.token cb token=\(String(describing: token)) error=\(String(describing: error))")
              guard let token = token else { return }
              self.fcmToken = token
              if let invoke = self.registerInvoke {
                var data = JSObject()
                data["token"] = token
                invoke.resolve(data)
                self.registerInvoke = nil
              }
            }
          }
        }
      )
    }
  }

  // Install forwarders for the two APNS UIApplicationDelegate methods.
  // Captures the previous IMP per selector and chains to it, so Firebase's
  // ISA-swizzle (when enabled) or any other plugin's swizzle still runs.
  private func installApnsDelegateSwizzle() {
    guard !NotificationPlugin.apnsHookInstalled else { return }
    guard let delegate = UIApplication.shared.delegate else {
      Logger.error("No UIApplication.delegate at load, cannot install APNS swizzle")
      return
    }
    NotificationPlugin.apnsHookInstalled = true

    let delegateClass: AnyClass = type(of: delegate)
    let className = String(cString: class_getName(delegateClass))
    Logger.info("Installing APNS swizzle on \(className)")

    // didRegisterForRemoteNotificationsWithDeviceToken
    let registerSel = #selector(UIApplicationDelegate.application(_:didRegisterForRemoteNotificationsWithDeviceToken:))
    typealias RegisterFn = @convention(c) (AnyObject, Selector, UIApplication, Data) -> Void
    let previousRegisterIMP: IMP? = class_getInstanceMethod(delegateClass, registerSel).map { method_getImplementation($0) }
    let registerBlock: @convention(block) (AnyObject, UIApplication, Data) -> Void = { selfObj, app, token in
      Logger.info("APNS swizzle: received token length=\(token.count)")
      // Set apnsToken so Firebase can exchange APNS → FCM once allowed.
      // We intentionally do NOT flip isAutoInitEnabled here — that's reserved
      // for registerForPushNotifications so that no FCM fetch (and no network
      // call to Google) happens before the user consents.
      Messaging.messaging().apnsToken = token
      // Chain to any previously installed handler.
      if let prev = previousRegisterIMP {
        unsafeBitCast(prev, to: RegisterFn.self)(selfObj, registerSel, app, token)
      }
    }
    let registerIMP = imp_implementationWithBlock(registerBlock)
    if let method = class_getInstanceMethod(delegateClass, registerSel) {
      method_setImplementation(method, registerIMP)
    } else {
      class_addMethod(delegateClass, registerSel, registerIMP, "v@:@@")
    }

    // didFailToRegisterForRemoteNotificationsWithError
    let failSel = #selector(UIApplicationDelegate.application(_:didFailToRegisterForRemoteNotificationsWithError:))
    typealias FailFn = @convention(c) (AnyObject, Selector, UIApplication, NSError) -> Void
    let previousFailIMP: IMP? = class_getInstanceMethod(delegateClass, failSel).map { method_getImplementation($0) }
    let failBlock: @convention(block) (AnyObject, UIApplication, NSError) -> Void = { [weak self] selfObj, app, error in
      Logger.error("APNS swizzle: registration failed: \(error.localizedDescription)")
      if let invoke = self?.registerInvoke {
        invoke.reject(error.localizedDescription)
        self?.registerInvoke = nil
      }
      if let prev = previousFailIMP {
        unsafeBitCast(prev, to: FailFn.self)(selfObj, failSel, app, error)
      }
    }
    let failIMP = imp_implementationWithBlock(failBlock)
    if let method = class_getInstanceMethod(delegateClass, failSel) {
      method_setImplementation(method, failIMP)
    } else {
      class_addMethod(delegateClass, failSel, failIMP, "v@:@@")
    }
  }

  @objc public func show(_ invoke: Invoke) throws {
    let notification = try invoke.parseArgs(Notification.self)

    let request = try showNotification(invoke: invoke, notification: notification)
    notificationHandler.saveNotification(request.identifier, notification)
    invoke.resolve(Int(request.identifier) ?? -1)
  }

  @objc public func batch(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(BatchArgs.self)
    var ids = [Int]()

    for notification in args.notifications {
      let request = try showNotification(invoke: invoke, notification: notification)
      notificationHandler.saveNotification(request.identifier, notification)
      ids.append(Int(request.identifier) ?? -1)
    }

    invoke.resolve(ids)
  }

  @objc public override func requestPermissions(_ invoke: Invoke) {
    notificationHandler.requestPermissions { granted, error in
      guard error == nil else {
        invoke.reject(error!.localizedDescription)
        return
      }
      invoke.resolve(["permissionState": granted ? "granted" : "denied"])
    }
  }

  @objc public override func checkPermissions(_ invoke: Invoke) {
    notificationHandler.checkPermissions { status in
      let permission: String

      switch status {
      case .authorized, .ephemeral, .provisional:
        permission = "granted"
      case .denied:
        permission = "denied"
      case .notDetermined:
        permission = "prompt"
      @unknown default:
        permission = "prompt"
      }

      invoke.resolve(["permissionState": permission])
    }
  }

  @objc func cancel(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(CancelArgs.self)

    UNUserNotificationCenter.current().removePendingNotificationRequests(
      withIdentifiers: args.notifications.map { String($0) }
    )
    invoke.resolve()
  }

  @objc func getPending(_ invoke: Invoke) {
    UNUserNotificationCenter.current().getPendingNotificationRequests(completionHandler: {
      (notifications) in
      let ret = notifications.compactMap({ [weak self] (notification) -> PendingNotification? in
        return self?.notificationHandler.toPendingNotification(notification)
      })

      invoke.resolve(ret)
    })
  }

  @objc func registerActionTypes(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(RegisterActionTypesArgs.self)
    makeCategories(args.types)
    invoke.resolve()
  }

  @objc func removeActive(_ invoke: Invoke) {
    do {
      let args = try invoke.parseArgs(RemoveActiveArgs.self)
      UNUserNotificationCenter.current().removeDeliveredNotifications(
        withIdentifiers: args.notifications.map { String($0.id) })
      invoke.resolve()
    } catch {
      UNUserNotificationCenter.current().removeAllDeliveredNotifications()
      DispatchQueue.main.async(execute: {
        UIApplication.shared.applicationIconBadgeNumber = 0
      })
      invoke.resolve()
    }
  }

  @objc func getActive(_ invoke: Invoke) {
    UNUserNotificationCenter.current().getDeliveredNotifications(completionHandler: {
      (notifications) in
      let ret = notifications.map({ (notification) -> ActiveNotification in
        return self.notificationHandler.toActiveNotification(
          notification.request)
      })
      invoke.resolve(ret)
    })
  }

  @objc func createChannel(_ invoke: Invoke) {
    invoke.reject("not implemented")
  }

  @objc func deleteChannel(_ invoke: Invoke) {
    invoke.reject("not implemented")
  }

  @objc func listChannels(_ invoke: Invoke) {
    invoke.reject("not implemented")
  }

}

@_cdecl("init_plugin_notification")
func initPlugin() -> Plugin {
  // Configure Firebase at plugin load so its default app exists before any
  // Firebase subsystem queries it (avoids [FirebaseCore][I-COR000003]).
  // No network calls or identifiers are generated here: auto-init and data
  // collection are disabled via Info.plist (FirebaseMessagingAutoInitEnabled,
  // FirebaseDataCollectionDefaultEnabled) until the user consents in
  // registerForPushNotifications.
  if FirebaseApp.app() == nil {
    FirebaseApp.configure()
  }
  return NotificationPlugin()
}
