// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

import Foundation
import UserNotifications

@objc public protocol NotificationHandlerProtocol {
  func willPresent(
    notification: UNNotification,
    completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  )
  func didReceive(response: UNNotificationResponse)
}

@objc public class NotificationManager: NSObject, UNUserNotificationCenterDelegate {
  public weak var notificationHandler: NotificationHandlerProtocol?

  override init() {
    super.init()
    let center = UNUserNotificationCenter.current()
    center.delegate = self
  }

  public func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    if let handler = notificationHandler {
      handler.willPresent(notification: notification, completionHandler: completionHandler)
    } else {
      completionHandler([.alert, .sound])
    }
  }

  public func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    notificationHandler?.didReceive(response: response)
    completionHandler()
  }
}
