import Intents
import TauriNotificationServiceFFI
import UserNotifications
import os

private let log = Logger(
  subsystem: Bundle.main.bundleIdentifier ?? "tauri-plugin-notification",
  category: "service"
)

/// Read by the plugin's foreground handler: only notifications this extension
/// rendered are shown while the app is open. iOS delivers the raw push without
/// it when it kills the extension before it answers.
private let contentStatusKey = "__nse_content_status__"

/// The principal class of the app's notification service extension: runs the
/// app's `#[receive_push_notification]` handler on every push.
///
/// The handler's data dir is the App Group container the app shares with its
/// extensions: the `TauriAppGroup` key of the extension's Info.plist, or
/// `group.<app bundle id>` when it has none.
@objc(TauriNotificationService)
open class TauriNotificationService: UNNotificationServiceExtension {
  private var contentHandler: ((UNNotificationContent) -> Void)?
  private var bestAttemptContent: UNMutableNotificationContent?

  open override func didReceive(
    _ request: UNNotificationRequest,
    withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
  ) {
    guard let content = request.content.mutableCopy() as? UNMutableNotificationContent else {
      contentHandler(request.content)
      return
    }
    self.contentHandler = contentHandler
    self.bestAttemptContent = content

    guard let dataDir = appGroupContainer()?.path else {
      log.error("no App Group container: is the App Group in both targets' entitlements?")
      deliverFallback()
      return
    }
    guard let handled = receive(content, dataDir: dataDir) else {
      suppress()
      return
    }
    deliver(render(handled, onto: content), status: "delivered")
  }

  /// Called just before iOS's ~30s budget runs out. Without an answer iOS
  /// shows the raw push payload.
  open override func serviceExtensionTimeWillExpire() {
    log.error("the handler outlived the extension's time budget")
    deliverFallback()
  }

  private func receive(_ content: UNNotificationContent, dataDir: String) -> [String: Any]? {
    var payload: [String: Any] = ["title": content.title, "body": content.body]
    var extra = content.userInfo.filter { "\($0.key)" != "aps" }
    extra = extra.filter { JSONSerialization.isValidJSONObject([$0.value]) }
    payload["extra"] = Dictionary(uniqueKeysWithValues: extra.map { ("\($0.key)", $0.value) })
    guard let json = try? JSONSerialization.data(withJSONObject: payload),
      let jsonString = String(data: json, encoding: .utf8)
    else {
      return nil
    }
    guard let result = tauri_plugin_notification_receive_push(jsonString, dataDir) else {
      return nil
    }
    defer { tauri_plugin_notification_free_push(result) }
    let data = Data(String(cString: result).utf8)
    guard let handled = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
      return nil
    }
    let title = handled["title"] as? String ?? ""
    let body = handled["body"] as? String ?? ""
    return title.isEmpty && body.isEmpty ? nil : handled
  }

  private func render(_ handled: [String: Any], onto content: UNMutableNotificationContent)
    -> UNNotificationContent
  {
    let title = handled["title"] as? String ?? ""
    let body = handled["body"] as? String ?? ""
    let route = handled["route"] as? String
    content.title = title
    content.body = body
    if let route, !route.isEmpty {
      content.userInfo["__notification_route__"] = route
    }
    let avatar = (handled["largeIconBytes"] as? String).flatMap(decodeBase64DataURL)
    let style = handled["conversationStyle"] as? [String: Any]

    if let avatar, let route, !route.isEmpty, !title.isEmpty, !body.isEmpty,
      let communication = communicationContent(
        content,
        sender: (style?["senderId"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? route,
        conversation: route,
        displayName: title,
        body: body,
        avatar: avatar,
        conversationTitle: (style?["conversationTitle"] as? String).flatMap {
          $0.isEmpty ? nil : $0
        }
      )
    {
      return communication
    }
    if let avatar, let attachment = avatarAttachment(avatar) {
      content.attachments = [attachment]
    }
    return content
  }

  /// A Communication Notification: the sender's avatar takes the app icon's
  /// place. Needs the `usernotifications.communication` entitlement.
  private func communicationContent(
    _ base: UNMutableNotificationContent,
    sender: String,
    conversation: String,
    displayName: String,
    body: String,
    avatar: Data,
    conversationTitle: String?
  ) -> UNNotificationContent? {
    let person = INPerson(
      personHandle: INPersonHandle(value: sender, type: .unknown),
      nameComponents: nil,
      displayName: displayName,
      image: INImage(imageData: avatar),
      contactIdentifier: nil,
      customIdentifier: sender
    )
    let intent = INSendMessageIntent(
      recipients: nil,
      outgoingMessageType: .outgoingMessageText,
      content: body,
      speakableGroupName: conversationTitle.map { INSpeakableString(spokenPhrase: $0) },
      conversationIdentifier: conversation,
      serviceName: appName(),
      sender: person,
      attachments: nil
    )
    let interaction = INInteraction(intent: intent, response: nil)
    interaction.direction = .incoming
    interaction.donate(completion: nil)

    // `updating(from:)` does not always carry the sound over.
    let sound = base.sound ?? .default
    base.sound = sound
    do {
      let updated = try base.updating(from: intent)
      guard let mutable = updated.mutableCopy() as? UNMutableNotificationContent else {
        return updated
      }
      mutable.sound = sound
      return mutable
    } catch {
      log.error("updating(from: intent) failed: \(error, privacy: .public)")
      return nil
    }
  }

  private func avatarAttachment(_ bytes: Data) -> UNNotificationAttachment? {
    let url = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent(UUID().uuidString)
      .appendingPathExtension("png")
    do {
      try bytes.write(to: url, options: .atomic)
      return try UNNotificationAttachment(identifier: "avatar", url: url)
    } catch {
      log.error("avatar attachment failed: \(error, privacy: .public)")
      return nil
    }
  }

  /// A real message most likely arrived, so show a readable placeholder
  /// rather than the raw push payload.
  private func deliverFallback() {
    guard let content = bestAttemptContent else { return }
    content.title = NSLocalizedString("New message", comment: "")
    content.body = ""
    deliver(content, status: "fallback")
  }

  /// Shows nothing. Needs the `usernotifications.filtering` entitlement.
  private func suppress() {
    guard let contentHandler else { return }
    self.contentHandler = nil
    contentHandler(UNNotificationContent())
  }

  private func deliver(_ content: UNNotificationContent, status: String) {
    guard let contentHandler else { return }
    self.contentHandler = nil
    guard let mutable = content.mutableCopy() as? UNMutableNotificationContent else {
      contentHandler(content)
      return
    }
    mutable.userInfo[contentStatusKey] = status
    contentHandler(mutable)
  }
}

private func appGroupContainer() -> URL? {
  let group =
    Bundle.main.object(forInfoDictionaryKey: "TauriAppGroup") as? String
    ?? containingAppBundle()?.bundleIdentifier.map { "group.\($0)" }
  return group.flatMap {
    FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: $0)
  }
}

/// Extensions live in `<App>.app/PlugIns/<Extension>.appex`.
private func containingAppBundle() -> Bundle? {
  Bundle(url: Bundle.main.bundleURL.deletingLastPathComponent().deletingLastPathComponent())
}

private func appName() -> String? {
  let info = containingAppBundle()?.infoDictionary
  return info?["CFBundleDisplayName"] as? String ?? info?["CFBundleName"] as? String
}

/// Accepts a `data:<mime>;base64,` URL or bare base64.
private func decodeBase64DataURL(_ s: String) -> Data? {
  let base64 = s.range(of: "base64,").map { String(s[$0.upperBound...]) } ?? s
  return Data(base64Encoded: base64)
}

/// Referenced by the extension target's C stub so the linker keeps this file:
/// nothing else references it, and iOS looks its class up by name.
@_cdecl("tauri_notification_service")
public func tauriNotificationService() {}
