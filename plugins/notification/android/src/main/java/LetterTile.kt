// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

package app.tauri.notification

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Rect
import android.text.TextPaint
import androidx.core.content.ContextCompat
import app.tauri.annotation.InvokeArg

private const val SIZE_PX = 192
private const val TEXT_HEIGHT = 0.4f
/// The system crops the icon to a circle, so wider text shrinks to stay inside it.
private const val MAX_TEXT_WIDTH = 0.7f
private const val SILHOUETTE_SIZE = 0.6f

/// Text centred on a coloured square, drawn with the system fonts so any
/// script or emoji renders. A person silhouette takes the place of a blank text.
@InvokeArg
class LetterTile {
  var text: String? = null
  var backgroundColor: String = "#8e8e93"
  var textColor: String = "#ffffff"

  fun toBitmap(context: Context): Bitmap {
    val bitmap = Bitmap.createBitmap(SIZE_PX, SIZE_PX, Bitmap.Config.ARGB_8888)
    val canvas = Canvas(bitmap)
    canvas.drawColor(Color.parseColor(backgroundColor))
    val foreground = Color.parseColor(textColor)
    val letters = text?.takeIf { it.isNotBlank() }
    if (letters == null) {
      drawSilhouette(context, canvas, foreground)
    } else {
      drawLetters(canvas, letters, foreground)
    }
    return bitmap
  }

  private fun drawLetters(canvas: Canvas, letters: String, color: Int) {
    val paint = TextPaint(Paint.ANTI_ALIAS_FLAG)
    paint.color = color
    paint.textAlign = Paint.Align.CENTER
    paint.textSize = SIZE_PX * TEXT_HEIGHT
    val width = paint.measureText(letters)
    if (width > SIZE_PX * MAX_TEXT_WIDTH) {
      paint.textSize *= SIZE_PX * MAX_TEXT_WIDTH / width
    }
    val bounds = Rect()
    paint.getTextBounds(letters, 0, letters.length, bounds)
    canvas.drawText(letters, SIZE_PX / 2f, SIZE_PX / 2f - bounds.exactCenterY(), paint)
  }

  private fun drawSilhouette(context: Context, canvas: Canvas, color: Int) {
    val silhouette = ContextCompat.getDrawable(context, R.drawable.notification_letter_tile_person)!!.mutate()
    silhouette.setTint(color)
    val inset = (SIZE_PX * (1 - SILHOUETTE_SIZE) / 2).toInt()
    silhouette.setBounds(inset, inset, SIZE_PX - inset, SIZE_PX - inset)
    silhouette.draw(canvas)
  }
}
