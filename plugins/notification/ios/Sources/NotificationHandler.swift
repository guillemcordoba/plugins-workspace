// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

import Tauri
import UIKit
import UserNotifications
import WebKit

/// userInfo key carrying the route a notification is associated with.
/// Set by the NSE from `NotificationData.route` (Rust). The plugin uses it to
/// suppress the foreground banner when the user is already on that route, and
/// to navigate the webview to it when the notification is tapped.
private let NOTIFICATION_ROUTE_USER_INFO_KEY = "__notification_route__"

public class NotificationHandler: NSObject, NotificationHandlerProtocol {

  public weak var plugin: Plugin?
  public weak var webView: WKWebView? {
    didSet {
      applyPendingRouteIfNeeded()
      observeWebViewForClearing()
    }
  }

  private var notificationsMap = [String: Notification]()
  /// Route requested by a tap before the webview was ready (app launched by
  /// tapping a notification from terminated state). Applied as soon as the
  /// webview is attached and has a URL.
  private var pendingRoute: String?
  private var pendingRouteUrlObservation: NSKeyValueObservation?
  /// Persistent KVO observer on the webview URL: when the user navigates to a
  /// route, any delivered notification for that route is dismissed (the mirror
  /// of the foreground banner-suppression in `willPresent`).
  private var clearRouteUrlObservation: NSKeyValueObservation?
  private var didBecomeActiveObserver: NSObjectProtocol?

  deinit {
    clearRouteUrlObservation?.invalidate()
    pendingRouteUrlObservation?.invalidate()
    if let observer = didBecomeActiveObserver {
      NotificationCenter.default.removeObserver(observer)
    }
  }

  internal func saveNotification(_ key: String, _ notification: Notification) {
    notificationsMap.updateValue(notification, forKey: key)
  }

  public func requestPermissions(with completion: ((Bool, Error?) -> Void)? = nil) {
    let center = UNUserNotificationCenter.current()
    center.requestAuthorization(options: [.badge, .alert, .sound]) { (granted, error) in
      completion?(granted, error)
    }
  }

  public func checkPermissions(with completion: ((UNAuthorizationStatus) -> Void)? = nil) {
    let center = UNUserNotificationCenter.current()
    center.getNotificationSettings { settings in
      completion?(settings.authorizationStatus)
    }
  }

  /// Mirrors the Android `isViewingRoute` check and the iOS `willPresent`
  /// suppression: the user is "viewing" the route iff the app is active in
  /// the foreground and the webview is on that path. Used by the `show`
  /// command to skip posting a banner when the user is already on the
  /// relevant screen. Calls `completion` on the main thread.
  public func isViewingRoute(_ route: String, completion: @escaping (Bool) -> Void) {
    DispatchQueue.main.async { [weak self] in
      guard let webView = self?.webView else {
        completion(false)
        return
      }
      if UIApplication.shared.applicationState != .active {
        completion(false)
        return
      }
      webView.evaluateJavaScript("window.location.pathname") { result, _ in
        completion((result as? String) == route)
      }
    }
  }

  public func willPresent(
    notification: UNNotification,
    completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    let notificationData = toActiveNotification(notification.request)
    try? self.plugin?.trigger("notification", data: notificationData)

    if let options = notificationsMap[notification.request.identifier] {
      if options.silent ?? false {
        completionHandler([])
        return
      }
    }

    let defaultOptions: UNNotificationPresentationOptions = [.badge, .sound, .alert]

    let route = notification.request.content.userInfo[NOTIFICATION_ROUTE_USER_INFO_KEY] as? String
    guard let route, !route.isEmpty, let webView = self.webView else {
      completionHandler(defaultOptions)
      return
    }

    DispatchQueue.main.async {
      webView.evaluateJavaScript("window.location.pathname") { result, _ in
        if let path = result as? String, path == route {
          completionHandler([])
        } else {
          completionHandler(defaultOptions)
        }
      }
    }
  }

  public func didReceive(response: UNNotificationResponse) {
    let originalNotificationRequest = response.notification.request
    let actionId = response.actionIdentifier

    var actionIdValue: String
    // We turn the two default actions (open/dismiss) into generic strings
    if actionId == UNNotificationDefaultActionIdentifier {
      actionIdValue = "tap"
    } else if actionId == UNNotificationDismissActionIdentifier {
      actionIdValue = "dismiss"
    } else {
      actionIdValue = actionId
    }

    var inputValue: String? = nil
    // If the type of action was for an input type, get the value
    if let inputType = response as? UNTextInputNotificationResponse {
      inputValue = inputType.userText
    }

    try? self.plugin?.trigger(
      "actionPerformed",
      data: ReceivedNotification(
        actionId: actionIdValue,
        inputValue: inputValue,
        notification: toActiveNotification(originalNotificationRequest)
    ))

    // Navigate the webview to the route this notification carries (taps only —
    // dismiss shouldn't navigate).
    if actionIdValue == "tap" {
      let route = originalNotificationRequest.content.userInfo[NOTIFICATION_ROUTE_USER_INFO_KEY] as? String
      if let route, !route.isEmpty {
        navigateWebView(to: route)
        clearDeliveredNotifications(forRoute: route)
      }
    }
  }

  private func navigateWebView(to route: String) {
    DispatchQueue.main.async { [weak self] in
      guard let self = self else { return }
      self.pendingRoute = route
      self.applyPendingRouteIfNeeded()
    }
  }

  /// Apply a pending tap-route to the webview if both are ready. If the webview
  /// is attached but its URL is still nil (the initial page hasn't begun
  /// loading yet — typical when the app was launched by tapping a notification
  /// from terminated state), install a one-shot KVO observer on `url` and
  /// re-run when it becomes available.
  private func applyPendingRouteIfNeeded() {
    guard let route = pendingRoute else { return }
    guard let webView = self.webView else { return }
    if let currentURL = webView.url,
       var components = URLComponents(url: currentURL, resolvingAgainstBaseURL: false) {
      components.path = route
      if let newURL = components.url {
        webView.load(URLRequest(url: newURL))
      }
      pendingRoute = nil
      pendingRouteUrlObservation?.invalidate()
      pendingRouteUrlObservation = nil
    } else if pendingRouteUrlObservation == nil {
      pendingRouteUrlObservation = webView.observe(\.url, options: .new) { [weak self] _, _ in
        DispatchQueue.main.async { self?.applyPendingRouteIfNeeded() }
      }
    }
  }

  /// Watch the webview URL and the app becoming active so notifications for the
  /// route the user is now viewing get dismissed. WKWebView updates `url` on
  /// History-API navigations (the SPA router's pushState/replaceState/popstate),
  /// so this fires on in-app route changes too.
  private func observeWebViewForClearing() {
    clearRouteUrlObservation?.invalidate()
    clearRouteUrlObservation = nil
    guard let webView = self.webView else { return }

    clearRouteUrlObservation = webView.observe(\.url, options: [.new]) { [weak self] _, _ in
      self?.clearNotificationsForCurrentRoute()
    }

    if didBecomeActiveObserver == nil {
      didBecomeActiveObserver = NotificationCenter.default.addObserver(
        forName: UIApplication.didBecomeActiveNotification,
        object: nil,
        queue: .main
      ) { [weak self] _ in
        self?.clearNotificationsForCurrentRoute()
      }
    }
  }

  /// Dismiss every delivered notification whose route matches the path the
  /// webview is currently showing.
  func clearNotificationsForCurrentRoute() {
    DispatchQueue.main.async { [weak self] in
      guard let webView = self?.webView else { return }
      webView.evaluateJavaScript("window.location.pathname") { result, _ in
        if let path = result as? String, !path.isEmpty {
          self?.clearDeliveredNotifications(forRoute: path)
        }
      }
    }
  }

  func clearDeliveredNotifications(forRoute route: String) {
    let center = UNUserNotificationCenter.current()
    center.getDeliveredNotifications { delivered in
      let ids = delivered
        .filter {
          ($0.request.content.userInfo[NOTIFICATION_ROUTE_USER_INFO_KEY] as? String) == route
        }
        .map { $0.request.identifier }
      if !ids.isEmpty {
        center.removeDeliveredNotifications(withIdentifiers: ids)
      }
    }
  }

  func toActiveNotification(_ request: UNNotificationRequest) -> ActiveNotification {
    let notificationRequest = notificationsMap[request.identifier]
    let threadIdentifier = request.content.threadIdentifier
    return ActiveNotification(
      id: Int(request.identifier) ?? -1,
      title: request.content.title,
      body: request.content.body,
      sound: notificationRequest?.sound ?? "",
      actionTypeId: request.content.categoryIdentifier,
      attachments: notificationRequest?.attachments,
      group: threadIdentifier.isEmpty ? nil : threadIdentifier
    )
  }

  func toPendingNotification(_ request: UNNotificationRequest) -> PendingNotification {
    return PendingNotification(
      id: Int(request.identifier) ?? -1,
      title: request.content.title,
      body: request.content.body
    )
  }
}

struct PendingNotification: Encodable {
  let id: Int
  let title: String
  let body: String
}

struct ActiveNotification: Encodable {
  let id: Int
  let title: String
  let body: String
  let sound: String
  let actionTypeId: String
  let attachments: [NotificationAttachment]?
  let group: String?
}

struct ReceivedNotification: Encodable {
  let actionId: String
  let inputValue: String?
  let notification: ActiveNotification
}
