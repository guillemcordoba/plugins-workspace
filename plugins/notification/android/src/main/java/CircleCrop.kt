// Copyright 2019-2023 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

package app.tauri.notification

import android.graphics.Bitmap
import android.graphics.BitmapShader
import android.graphics.Canvas
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Shader

/// The largest centred circle of this bitmap, on a transparent square. Some
/// launchers show a notification's large icon uncropped, so a square avatar
/// would show its corners; Signal circle-crops its avatars for the same reason.
fun Bitmap.circleCropped(): Bitmap {
  val size = minOf(width, height)
  val output = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
  val shader = BitmapShader(this, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP)
  shader.setLocalMatrix(Matrix().apply {
    setTranslate(-(width - size) / 2f, -(height - size) / 2f)
  })
  val paint = Paint(Paint.ANTI_ALIAS_FLAG)
  paint.shader = shader
  Canvas(output).drawCircle(size / 2f, size / 2f, size / 2f, paint)
  return output
}
