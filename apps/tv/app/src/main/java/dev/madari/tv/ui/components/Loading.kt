package dev.madari.tv.ui.components

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.madari.tv.ui.theme.TvColors

/** Startup surface that holds focus until real catalog items arrive. */
@Composable
fun HomeLoadingScreen(modifier: Modifier = Modifier) {
    val focus = remember { FocusRequester() }
    val motion = rememberInfiniteTransition(label = "home loading")
    val sweep = motion.animateFloat(
        initialValue = -0.4f,
        targetValue = 1.4f,
        animationSpec = infiniteRepeatable(tween(1800, easing = LinearEasing)),
        label = "loading sweep"
    )
    Box(modifier.fillMaxSize().background(TvColors.Background).focusRequester(focus).focusable()) {
        Canvas(Modifier.fillMaxSize()) {
            drawRect(Brush.radialGradient(
                colors = listOf(Color(0xFF21151C), TvColors.Background),
                center = center,
                radius = size.minDimension * 0.75f
            ))
        }
        Column(
            Modifier.align(Alignment.Center),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            BrandLogo(Modifier.size(76.dp))
            Spacer(Modifier.height(22.dp))
            Text("MADARI", color = Color.White, style = MaterialTheme.typography.headlineMedium.copy(
                fontWeight = FontWeight.Medium, letterSpacing = 7.sp, fontSize = 28.sp
            ))
            Spacer(Modifier.height(40.dp))
            Canvas(Modifier.size(144.dp, 2.dp).clip(RoundedCornerShape(1.dp)).semantics {
                contentDescription = "Loading home"
                progressBarRangeInfo = ProgressBarRangeInfo.Indeterminate
            }) {
                drawRect(Color.White.copy(alpha = 0.09f))
                val position = sweep.value * size.width
                val halfWidth = size.width * 0.4f
                drawRect(Brush.horizontalGradient(
                    colors = listOf(Color.Transparent, Color.White.copy(alpha = 0.85f), Color.Transparent),
                    startX = position - halfWidth,
                    endX = position + halfWidth
                ))
            }
            Spacer(Modifier.height(18.dp))
            Text("Loading your home", color = TvColors.Muted, style = MaterialTheme.typography.bodyMedium)
        }
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
}
