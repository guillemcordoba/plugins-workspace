// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

import Intents
import Tauri
import UserNotifications

enum NotificationError: LocalizedError {
  case triggerRepeatIntervalTooShort
  case attachmentFileNotFound(path: String)
  case attachmentUnableToCreate(String)
  case pastScheduledTime
  case invalidDate(String)

  var errorDescription: String? {
    switch self {
    case .triggerRepeatIntervalTooShort:
      return "Schedule interval too short, must be a least 1 minute"
    case .attachmentFileNotFound(let path):
      return "Unable to find file \(path) for attachment"
    case .attachmentUnableToCreate(let error):
      return "Failed to create attachment: \(error)"
    case .pastScheduledTime:
      return "Scheduled time must be *after* current time"
    case .invalidDate(let date):
      return "Could not parse date \(date)"
    }
  }
}

func makeNotificationContent(_ notification: Notification, pluginConfig: NotificationPluginConfig?) throws -> UNNotificationContent {
  let content = UNMutableNotificationContent()
  content.title = NSString.localizedUserNotificationString(
    forKey: notification.title, arguments: nil)
  if let body = notification.body {
    content.body = NSString.localizedUserNotificationString(
      forKey: body,
      arguments: nil)
  }

  if #available(iOS 15.0, *) {
    switch pluginConfig?.priority?.lowercased() {
    case "min", "low": content.interruptionLevel = .passive
    case nil, "default": content.interruptionLevel = .active
    case "high", "max": content.interruptionLevel = .timeSensitive
    default: content.interruptionLevel = .active
    }
  }

  var userInfo: [String: Any] = [:]

  if let extra = notification.extra {
    userInfo["__EXTRA__"] = extra
  }

  if let schedule = notification.schedule {
    userInfo["__SCHEDULE__"] = scheduleToDictionary(schedule)
  }

  if let route = notification.route, !route.isEmpty {
    userInfo["__notification_route__"] = route
  }

  content.userInfo = userInfo

  if let actionTypeId = notification.actionTypeId {
    content.categoryIdentifier = actionTypeId
  }

  if let threadIdentifier = notification.group {
    content.threadIdentifier = threadIdentifier
  }

  if let summaryArgument = notification.summary {
    content.summaryArgument = summaryArgument
  }

  if let sound = notification.sound ?? pluginConfig?.sound {
    content.sound = sound == "default"
      ? UNNotificationSound.default
      : UNNotificationSound(named: UNNotificationSoundName(sound))
  }

  if let attachments = notification.attachments {
    content.attachments = try makeAttachments(attachments)
  }

  // iOS 15+ Communication Notifications: build an INSendMessageIntent so
  // the sender's avatar replaces the app icon (matching the NSE /
  // background-push path). Skipped on iOS < 15 or when any required input
  // is missing — those cases deliver a plain title/body banner.
  if #available(iOS 15.0, *),
     notification.conversationStyle != nil,
     let route = notification.route, !route.isEmpty,
     let body = notification.body, !body.isEmpty,
     let iconBytes = notification.largeIconBytes, !iconBytes.isEmpty,
     let avatarData = decodeBase64DataURL(iconBytes) {
    let senderHandle = notification.conversationStyle?.senderId?.nilIfEmpty ?? route
    if let updated = communicationNotificationContent(
      from: content,
      senderHandle: senderHandle,
      conversationId: route,
      displayName: notification.title,
      body: body,
      avatarData: avatarData
    ) {
      return updated
    }
  }

  return content
}

/// Decodes a base64 data-URL (e.g. `data:image/png;base64,…`) or a bare
/// base64 string into raw bytes.
private func decodeBase64DataURL(_ s: String) -> Data? {
  let stripped: String
  if let range = s.range(of: "base64,") {
    stripped = String(s[range.upperBound...])
  } else {
    stripped = s
  }
  return Data(base64Encoded: stripped)
}

private extension String {
  var nilIfEmpty: String? { isEmpty ? nil : self }
}

/// Build an `INSendMessageIntent` for the incoming message and use
/// `UNMutableNotificationContent.updating(from:)` to produce a
/// Communication Notification with the avatar in the sender slot.
/// Mirrors the NSE's intent-construction logic so the in-app foreground
/// and background-push paths render the same notification.
@available(iOS 15.0, *)
private func communicationNotificationContent(
  from base: UNMutableNotificationContent,
  senderHandle: String,
  conversationId: String,
  displayName: String,
  body: String,
  avatarData: Data
) -> UNNotificationContent? {
  let avatar = INImage(imageData: avatarData)
  let handle = INPersonHandle(value: senderHandle, type: .unknown)
  let sender = INPerson(
    personHandle: handle,
    nameComponents: nil,
    displayName: displayName,
    image: avatar,
    contactIdentifier: nil,
    customIdentifier: senderHandle
  )

  let intent = INSendMessageIntent(
    recipients: nil,
    outgoingMessageType: .outgoingMessageText,
    content: body,
    speakableGroupName: nil,
    conversationIdentifier: conversationId,
    serviceName: nil,
    sender: sender,
    attachments: nil
  )

  let interaction = INInteraction(intent: intent, response: nil)
  interaction.direction = .incoming
  interaction.donate(completion: nil)

  // `updating(from:)` returns fresh content derived from the intent and
  // doesn't always carry the base's sound through; re-pin it on the
  // result so the notification doesn't fall back to silent.
  let desiredSound: UNNotificationSound? = base.sound
  do {
    let updated = try base.updating(from: intent)
    if let mutable = updated.mutableCopy() as? UNMutableNotificationContent {
      mutable.sound = desiredSound
      return mutable
    }
    return updated
  } catch {
    Logger.error("updating(from: intent) failed: \(error.localizedDescription)")
    return nil
  }
}

func scheduleToDictionary(_ schedule: NotificationSchedule) -> [String: Any] {
  switch schedule {
  case .at(let date, let repeating):
    return [
      "type": "at",
      "date": date,
      "repeating": repeating
    ]
  case .interval(let interval):
    return [
      "type": "interval",
      "interval": scheduleIntervalToDictionary(interval)
    ]
  case .every(let interval, let count):
    return [
      "type": "every",
      "interval": interval.rawValue,
      "count": count
    ]
  }
}

func scheduleIntervalToDictionary(_ interval: ScheduleInterval) -> [String: Any] {
  var dict: [String: Any] = [:]
  
  if let year = interval.year {
    dict["year"] = year
  }
  if let month = interval.month {
    dict["month"] = month
  }
  if let day = interval.day {
    dict["day"] = day
  }
  if let weekday = interval.weekday {
    dict["weekday"] = weekday
  }
  if let hour = interval.hour {
    dict["hour"] = hour
  }
  if let minute = interval.minute {
    dict["minute"] = minute
  }
  if let second = interval.second {
    dict["second"] = second
  }
  
  return dict
}

func makeAttachments(_ attachments: [NotificationAttachment]) throws -> [UNNotificationAttachment] {
  var createdAttachments = [UNNotificationAttachment]()

  for attachment in attachments {

    guard let urlObject = makeAttachmentUrl(attachment.url) else {
      throw NotificationError.attachmentFileNotFound(path: attachment.url)
    }

    let options = attachment.options != nil ? makeAttachmentOptions(attachment.options!) : nil

    do {
      let newAttachment = try UNNotificationAttachment(
        identifier: attachment.id, url: urlObject, options: options)
      createdAttachments.append(newAttachment)
    } catch {
      throw NotificationError.attachmentUnableToCreate(error.localizedDescription)
    }
  }

  return createdAttachments
}

func makeAttachmentUrl(_ path: String) -> URL? {
  return URL(string: path)
}

func makeAttachmentOptions(_ options: NotificationAttachmentOptions) -> [AnyHashable: Any] {
  var opts: [AnyHashable: Any] = [:]

  if let value = options.iosUNNotificationAttachmentOptionsTypeHintKey {
    opts[UNNotificationAttachmentOptionsTypeHintKey] = value
  }
  if let value = options.iosUNNotificationAttachmentOptionsThumbnailHiddenKey {
    opts[UNNotificationAttachmentOptionsThumbnailHiddenKey] = value
  }
  if let value = options.iosUNNotificationAttachmentOptionsThumbnailClippingRectKey {
    opts[UNNotificationAttachmentOptionsThumbnailClippingRectKey] = value
  }
  if let value = options
    .iosUNNotificationAttachmentOptionsThumbnailTimeKey

  {
    opts[UNNotificationAttachmentOptionsThumbnailTimeKey] = value
  }
  return opts
}

func handleScheduledNotification(_ schedule: NotificationSchedule) throws
  -> UNNotificationTrigger?
{
  switch schedule {
  case .at(let date, let repeating):
    let dateFormatter = DateFormatter()
    dateFormatter.locale = Locale(identifier: "en_US_POSIX")
    dateFormatter.dateFormat = "yyyy-MM-dd'T'HH:mm:ss.SSS'Z'"

    if let at = dateFormatter.date(from: date) {
      let dateInfo = Calendar.current.dateComponents(in: TimeZone.current, from: at)

      if dateInfo.date! < Date() {
        throw NotificationError.pastScheduledTime
      }

      let dateInterval = DateInterval(start: Date(), end: dateInfo.date!)

      // Notifications that repeat have to be at least a minute between each other
      if repeating && dateInterval.duration < 60 {
        throw NotificationError.triggerRepeatIntervalTooShort
      }

      return UNTimeIntervalNotificationTrigger(
        timeInterval: dateInterval.duration, repeats: repeating)

    } else {
      throw NotificationError.invalidDate(date)
    }
  case .interval(let interval):
    let dateComponents = getDateComponents(interval)
    return UNCalendarNotificationTrigger(dateMatching: dateComponents, repeats: true)
  case .every(let interval, let count):
    if let repeatDateInterval = getRepeatDateInterval(interval, count) {
      // Notifications that repeat have to be at least a minute between each other
      if repeatDateInterval.duration < 60 {
        throw NotificationError.triggerRepeatIntervalTooShort
      }

      return UNTimeIntervalNotificationTrigger(
        timeInterval: repeatDateInterval.duration, repeats: true)
    }
  }

  return nil
}

/// Given our schedule format, return a DateComponents object
/// that only contains the components passed in.

func getDateComponents(_ at: ScheduleInterval) -> DateComponents {
  // var dateInfo = Calendar.current.dateComponents(in: TimeZone.current, from: Date())
  // dateInfo.calendar = Calendar.current
  var dateInfo = DateComponents()

  if let year = at.year {
    dateInfo.year = year
  }
  if let month = at.month {
    dateInfo.month = month
  }
  if let day = at.day {
    dateInfo.day = day
  }
  if let hour = at.hour {
    dateInfo.hour = hour
  }
  if let minute = at.minute {
    dateInfo.minute = minute
  }
  if let second = at.second {
    dateInfo.second = second
  }
  if let weekday = at.weekday {
    dateInfo.weekday = weekday
  }
  return dateInfo
}

/// Compute the difference between the string representation of a date
/// interval and today. For example, if every is "month", then we
/// return the interval between today and a month from today.

func getRepeatDateInterval(_ every: ScheduleEveryKind, _ count: Int) -> DateInterval? {
  let cal = Calendar.current
  let now = Date()
  switch every {
  case .year:
    let newDate = cal.date(byAdding: .year, value: count, to: now)!
    return DateInterval(start: now, end: newDate)
  case .month:
    let newDate = cal.date(byAdding: .month, value: count, to: now)!
    return DateInterval(start: now, end: newDate)
  case .twoWeeks:
    let newDate = cal.date(byAdding: .weekOfYear, value: 2 * count, to: now)!
    return DateInterval(start: now, end: newDate)
  case .week:
    let newDate = cal.date(byAdding: .weekOfYear, value: count, to: now)!
    return DateInterval(start: now, end: newDate)
  case .day:
    let newDate = cal.date(byAdding: .day, value: count, to: now)!
    return DateInterval(start: now, end: newDate)
  case .hour:
    let newDate = cal.date(byAdding: .hour, value: count, to: now)!
    return DateInterval(start: now, end: newDate)
  case .minute:
    let newDate = cal.date(byAdding: .minute, value: count, to: now)!
    return DateInterval(start: now, end: newDate)
  case .second:
    let newDate = cal.date(byAdding: .second, value: count, to: now)!
    return DateInterval(start: now, end: newDate)
  }
}
