//
//  NotificationService.swift
//  PushNotificationsExtension
//
//  Created by Guillem Córdoba on 12/12/23.
//

import UserNotifications

func makeCString(from str: String) -> UnsafeMutablePointer<UInt8> {
    var utf8 = Array(str.utf8)
    utf8.append(0)  // adds null character
    let count = utf8.count
    let result = UnsafeMutableBufferPointer<UInt8>.allocate(capacity: count)
    _ = result.initialize(from: utf8)
    return result.baseAddress!
}

extension RustByteSlice {
    func asUnsafeBufferPointer() -> UnsafeBufferPointer<UInt8> {
        return UnsafeBufferPointer(start: bytes, count: len)
    }

    func asString() -> String? {
        return String(bytes: asUnsafeBufferPointer(), encoding: .utf8)
    }
}

struct Notification: Codable {
    let title: String
    let body: String
}

class NotificationService: UNNotificationServiceExtension {

    override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
        let bestAttemptContent = (request.content.mutableCopy() as? UNMutableNotificationContent)
        
        if let bestAttemptContent = bestAttemptContent {
            let s = "{ \"title\": \"\(bestAttemptContent.title)\", \"body\": \"\(bestAttemptContent.body)\" }"

            let slice = RustByteSlice(bytes: makeCString(from: s), len: s.count)

            let n = modify_notification(slice)

            // Modify the notification content here...
            bestAttemptContent.title = notification_title(n).asString()!
            bestAttemptContent.body = notification_body(n).asString()!
            
            notification_destroy(n)

            contentHandler(bestAttemptContent)
        }

    }
    
    override func serviceExtensionTimeWillExpire() {
        // Called just before the extension will be terminated by the system.
        // Use this as an opportunity to deliver your "best attempt" at modified content, otherwise the original push payload will be used.
       // if let contentHandler = contentHandler, let bestAttemptContent =  bestAttemptContent {
       //     contentHandler(bestAttemptContent)
       // }
    }

}
