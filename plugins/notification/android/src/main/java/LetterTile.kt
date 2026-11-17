// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

package app.tauri.notification

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Rect
import android.text.TextPaint
import app.tauri.annotation.InvokeArg

private const val SIZE_PX = 192
private const val TEXT_HEIGHT = 0.4f
/// The system crops the icon to a circle, so wider text shrinks to stay inside it.
private const val MAX_TEXT_WIDTH = 0.7f

/// Text centred on a coloured square, drawn with the system fonts so any
/// script or emoji renders.
@InvokeArg
class LetterTile {
  var text: String = ""
  var backgroundColor: String = "#8e8e93"
  var textColor: String = "#ffffff"

  fun toBitmap(): Bitmap {
    val bitmap = Bitmap.createBitmap(SIZE_PX, SIZE_PX, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(bitmap)
    canvas.drawColor(Color.parseColor(backgroundColor))
    val paint = TextPaint(Paint.ANTI_ALIAS_FLAG)
    paint.color = Color.parseColor(textColor)
    paint.textAlign = Paint.Align.CENTER
    paint.textSize = SIZE_PX * TEXT_HEIGHT
    val width = paint.measureText(text)
    if (width > SIZE_PX * MAX_TEXT_WIDTH) {
      paint.textSize *= SIZE_PX * MAX_TEXT_WIDTH / width
    }
    val bounds = Rect()
    paint.getTextBounds(text, 0, text.length, bounds)
    canvas.drawText(text, SIZE_PX / 2f, SIZE_PX / 2f - bounds.exactCenterY(), paint)
    return bitmap
  }
}
