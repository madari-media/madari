package dev.madari.tv.ui.theme

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.gestures.LocalBringIntoViewSpec
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.tv.material3.*
import dev.madari.tv.R

object TvColors {
    val Background = Color(0xFF0B0C0F)
    val Panel = Color(0xFF1C1E23)
    val Accent = Color(0xFFF05B64)
    val Muted = Color(0xFFAAADB5)
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun MadariTheme(content: @Composable () -> Unit) {
    val font = FontFamily(Font(R.font.dm_sans))
    val base = Typography()
    MaterialTheme(
        colorScheme = darkColorScheme(
            primary = TvColors.Accent, onPrimary = Color.White,
            background = TvColors.Background, surface = TvColors.Panel,
            onSurface = Color.White, onBackground = Color.White
        ),
        typography = Typography(
            displayLarge = base.displayLarge.copy(fontFamily = font),
            displayMedium = base.displayMedium.copy(fontFamily = font),
            headlineLarge = base.headlineLarge.copy(fontFamily = font),
            headlineMedium = base.headlineMedium.copy(fontFamily = font),
            titleLarge = base.titleLarge.copy(fontFamily = font),
            titleMedium = base.titleMedium.copy(fontFamily = font),
            bodyLarge = base.bodyLarge.copy(fontFamily = font),
            bodyMedium = base.bodyMedium.copy(fontFamily = font),
            labelLarge = base.labelLarge.copy(fontFamily = font)
        ),
        content = { CompositionLocalProvider(LocalContentColor provides Color.White, LocalBringIntoViewSpec provides TvFocusScroll, content = content) }
    )
}
