package dev.madari.tv.ui.components

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.dp
import androidx.tv.material3.LocalContentColor

/** Single-weight vector glyph set used across the TV interface. */
@Composable
fun Glyph(name: String, modifier: Modifier = Modifier, color: Color = LocalContentColor.current) {
    val path = remember(name) { PathParser().parsePathString(when(name) {
        "home" -> "M3,10L12,3L21,10V21H15V14H9V21H3Z"
        "explore" -> "M3,3H10V10H3Z M14,3H21V10H14Z M3,14H10V21H3Z M14,14H21V21H14Z"
        "calendar" -> "M3,5H21V21H3Z M3,10H21 M8,2V7 M16,2V7"
        "search" -> "M10.5,3a7.5,7.5 0,1 0,0,15a7.5,7.5 0,1 0,0,-15 M16,16L22,22"
        "play" -> "M7,4L20,12L7,20Z"
        "pause" -> "M8,4V20 M16,4V20"
        "next" -> "M5,5L15,12L5,19Z M19,5V19"
        "previous" -> "M19,5L9,12L19,19Z M5,5V19"
        "backward" -> "M11,5L3,12L11,19Z M21,5L13,12L21,19Z"
        "forward" -> "M3,5L11,12L3,19Z M13,5L21,12L13,19Z"
        "audio" -> "M4,9H8L14,4V20L8,15H4Z M18,8Q23,12 18,16"
        "subtitles" -> "M3,5H21V19H3Z M6,10H10 M14,10H18 M6,14H13 M16,14H18"
        "episodes" -> "M7,3H21V17 M3,7H17V21H3Z M7,11L13,14L7,17Z"
        "settings" -> "M4,5H20 M4,12H20 M4,19H20 M8,2V8 M16,9V15 M10,16V22"
        "screen" -> "M3,4H21V18H3Z M8,22H16"
        "source" -> "M4,5H20V10H4Z M4,14H20V19H4Z M7,7.5H8 M7,16.5H8"
        "speed" -> "M3,18A10,10 0,1 1,21,18 M12,14L17,7 M12,14h0.1"
        "check" -> "M4,12L9,17L20,6"
        "plus" -> "M12,4V20 M4,12H20"
        "info" -> "M12,2a10,10 0,1 0,0,20a10,10 0,1 0,0,-20 M12,10V17 M12,6V6.1"
        "close" -> "M5,5L19,19 M5,19L19,5"
        else -> "M5,5H19V19H5Z"
    }).toPath() }
    Canvas(modifier.size(22.dp)) {
        scale(size.width / 24f, size.height / 24f, pivot = androidx.compose.ui.geometry.Offset.Zero) {
            drawPath(path, color, style = Stroke(1.7f, cap = StrokeCap.Round, join = StrokeJoin.Round))
        }
    }
}
