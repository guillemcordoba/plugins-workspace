// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

import UIKit

/// Mirrors the Rust `LetterTile`: text centred on a coloured square, drawn
/// with the system font so any script or emoji renders, or a person
/// silhouette when the text is blank.
struct LetterTile: Decodable {
  var text: String?
  var backgroundColor: String
  var textColor: String

  func pngData() -> Data? {
    drawLetterTile(text: text, backgroundColor: backgroundColor, textColor: textColor)
  }
}

private let letterTileSize: CGFloat = 192
private let textHeight: CGFloat = 0.4
/// The system crops the avatar to a circle, so wider text shrinks to stay inside it.
private let maxTextWidth: CGFloat = 0.7
private let silhouetteSize: CGFloat = 0.5

/// Kept in sync with the copy in the app's notification service extension,
/// which cannot link this module.
func drawLetterTile(text: String?, backgroundColor: String, textColor: String) -> Data? {
  guard let background = UIColor(hexRGB: backgroundColor),
        let foreground = UIColor(hexRGB: textColor) else { return nil }
  let size = CGSize(width: letterTileSize, height: letterTileSize)
  let format = UIGraphicsImageRendererFormat()
  format.scale = 1
  let image = UIGraphicsImageRenderer(size: size, format: format).image { context in
    background.setFill()
    context.fill(CGRect(origin: .zero, size: size))
    let letters = text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    if letters.isEmpty {
      drawSilhouette(color: foreground)
    } else {
      drawLetters(letters, color: foreground)
    }
  }
  return image.pngData()
}

private func drawLetters(_ letters: String, color: UIColor) {
  var font = UIFont.systemFont(ofSize: letterTileSize * textHeight)
  let width = (letters as NSString).size(withAttributes: [.font: font]).width
  if width > letterTileSize * maxTextWidth {
    font = font.withSize(font.pointSize * letterTileSize * maxTextWidth / width)
  }
  let attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color]
  let textSize = (letters as NSString).size(withAttributes: attributes)
  let baseline = (letterTileSize + font.capHeight) / 2
  let origin = CGPoint(x: (letterTileSize - textSize.width) / 2, y: baseline - font.ascender)
  (letters as NSString).draw(at: origin, withAttributes: attributes)
}

/// Material Design Icons "account-outline", drawn from its 24pt path.
private func drawSilhouette(color: UIColor) {
  let path = UIBezierPath()
  path.append(UIBezierPath(arcCenter: CGPoint(x: 12, y: 8), radius: 4, startAngle: 0, endAngle: 2 * .pi, clockwise: true))
  path.append(UIBezierPath(arcCenter: CGPoint(x: 12, y: 8), radius: 2, startAngle: 0, endAngle: 2 * .pi, clockwise: true))
  path.move(to: CGPoint(x: 12, y: 13))
  path.addCurve(to: CGPoint(x: 20, y: 17), controlPoint1: CGPoint(x: 14.67, y: 13), controlPoint2: CGPoint(x: 20, y: 14.33))
  path.addLine(to: CGPoint(x: 20, y: 20))
  path.addLine(to: CGPoint(x: 4, y: 20))
  path.addLine(to: CGPoint(x: 4, y: 17))
  path.addCurve(to: CGPoint(x: 12, y: 13), controlPoint1: CGPoint(x: 4, y: 14.33), controlPoint2: CGPoint(x: 9.33, y: 13))
  path.close()
  path.move(to: CGPoint(x: 12, y: 14.9))
  path.addCurve(to: CGPoint(x: 5.9, y: 17), controlPoint1: CGPoint(x: 9.03, y: 14.9), controlPoint2: CGPoint(x: 5.9, y: 16.36))
  path.addLine(to: CGPoint(x: 5.9, y: 18.1))
  path.addLine(to: CGPoint(x: 18.1, y: 18.1))
  path.addLine(to: CGPoint(x: 18.1, y: 17))
  path.addCurve(to: CGPoint(x: 12, y: 14.9), controlPoint1: CGPoint(x: 18.1, y: 16.36), controlPoint2: CGPoint(x: 14.97, y: 14.9))
  path.close()
  path.usesEvenOddFillRule = true
  let iconSize = letterTileSize * silhouetteSize
  let inset = (letterTileSize - iconSize) / 2
  path.apply(CGAffineTransform(scaleX: iconSize / 24, y: iconSize / 24))
  path.apply(CGAffineTransform(translationX: inset, y: inset))
  color.setFill()
  path.fill()
}

private extension UIColor {
  convenience init?(hexRGB: String) {
    let hex = hexRGB.hasPrefix("#") ? String(hexRGB.dropFirst()) : hexRGB
    guard hex.count == 6, let value = UInt32(hex, radix: 16) else { return nil }
    self.init(
      red: CGFloat((value >> 16) & 0xff) / 255,
      green: CGFloat((value >> 8) & 0xff) / 255,
      blue: CGFloat(value & 0xff) / 255,
      alpha: 1
    )
  }
}
