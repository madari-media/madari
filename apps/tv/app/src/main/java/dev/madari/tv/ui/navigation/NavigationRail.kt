package dev.madari.tv.ui.navigation

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.madari.tv.ui.components.BrandLogo
import dev.madari.tv.ui.components.Glyph
import dev.madari.tv.ui.theme.TvColors

/**
 * Resolves the animated width during the layout pass. Reading the animation state here
 * instead of in composition keeps the rail (six tv-material buttons, glyphs and labels)
 * out of recomposition on every frame; only measure/layout re-runs.
 */
private fun Modifier.animatedWidth(width: () -> Dp) = layout { measurable, constraints ->
    val w = width().roundToPx()
    val placeable = measurable.measure(Constraints(minWidth = w, maxWidth = w, minHeight = constraints.minHeight, maxHeight = constraints.maxHeight))
    layout(w, placeable.height) { placeable.place(0, 0) }
}

/** Fixed content inset: opening the rail never remeasures the catalog underneath. */
@Composable
fun NavigationRail(selected: String, profile: String, onSelect: (String)->Unit, first: FocusRequester) {
    var expanded by remember { mutableStateOf(false) }
    var railFocused by remember { mutableStateOf(false) }
    var selections by remember { mutableStateOf(0) }
    val focus=LocalFocusManager.current
    val entries=remember { listOf("Search" to "search","Home" to "home","Explore" to "explore","My list" to "plus","Calendar" to "calendar","Settings" to "settings") }
    val itemFocus=remember { entries.associate { it.first to FocusRequester() } }
    // Directional focus search picks the geometrically nearest rail item, so returning to
    // the rail from the content landed on Search instead of the selected destination.
    LaunchedEffect(railFocused) { if(railFocused) itemFocus[selected]?.requestFocus() }
    // Selecting a destination changes the screen under the rail, so the old immediate
    // moveFocus(Right) landed on a node that was about to be disposed and focus fell back
    // to the rail's first item. Wait for the new screen to compose, then move only if that
    // screen did not already claim focus (e.g. the Home hero).
    LaunchedEffect(selections) {
        if(selections>0) {
            withFrameNanos { }
            if(railFocused) focus.moveFocus(FocusDirection.Right)
        }
    }
    val railWidth=animateDpAsState(if(expanded) 248.dp else 72.dp,tween(200,easing=FastOutSlowInEasing),label="rail width")
    val contentWidth=animateDpAsState(if(expanded) 206.dp else 72.dp,tween(200,easing=FastOutSlowInEasing),label="rail content width")
    val background=remember { Brush.horizontalGradient(listOf(TvColors.Background,TvColors.Background,Color.Transparent)) }
    Box(Modifier.animatedWidth { railWidth.value }.fillMaxHeight().clipToBounds().background(background)) {
        Column(Modifier.animatedWidth { contentWidth.value }.fillMaxHeight().padding(vertical=26.dp)
            .onFocusChanged { railFocused=it.hasFocus; expanded=it.hasFocus }.focusGroup(),horizontalAlignment=Alignment.Start) {
            BrandLogo(Modifier.padding(start=15.dp))
            Spacer(Modifier.weight(1f))
            entries.forEach { (tab,icon) ->
                var focused by remember { mutableStateOf(false) }
                Button(onClick={onSelect(tab);selections++},
                    modifier=Modifier.padding(start=11.dp,end=11.dp,bottom=9.dp).height(46.dp).fillMaxWidth()
                        .then(if(tab=="Home") Modifier.focusRequester(first) else Modifier)
                        .focusRequester(itemFocus.getValue(tab))
                        .onFocusChanged { focused=it.isFocused }.semantics { contentDescription=tab },
                    contentPadding=PaddingValues(horizontal=14.dp),
                    shape=ButtonDefaults.shape(shape=RoundedCornerShape(8.dp)),
                    scale=ButtonDefaults.scale(focusedScale=1f),
                    colors=ButtonDefaults.colors(containerColor=Color.Transparent,contentColor=if(selected==tab) Color.White else TvColors.Muted,focusedContainerColor=Color.White.copy(.12f),focusedContentColor=Color.White)) {
                    // tv-material's Button centers its content inside a 58dp minimum and
                    // places that 40dp-min row at the top, which shifted the glyph 4dp right
                    // on label removal and left the content 3dp high. Filling the button
                    // pins the content to the start and centres it vertically.
                    Row(Modifier.fillMaxWidth().fillMaxHeight(),horizontalArrangement=Arrangement.Start,verticalAlignment=Alignment.CenterVertically) {
                        Glyph(icon,Modifier.size(22.dp),if(focused || selected==tab) Color.White else TvColors.Muted)
                        AnimatedVisibility(visible=expanded,enter=fadeIn(tween(150,delayMillis=50)),exit=fadeOut(tween(90))) {
                            Row(verticalAlignment=Alignment.CenterVertically) {
                                Spacer(Modifier.width(20.dp))
                                // Unbounded width keeps the text constraints stable while the rail
                                // animates, so the paragraph is measured once instead of every frame.
                                Text(tab,Modifier.wrapContentWidth(unbounded=true),style=MaterialTheme.typography.titleMedium,maxLines=1)
                            }
                        }
                    }
                }
            }
            Spacer(Modifier.weight(1f))
            Crossfade(targetState=expanded,animationSpec=tween(200,easing=FastOutSlowInEasing),label="rail profile") { open ->
                Text(if(open) profile else profile.take(1).uppercase(),Modifier.padding(start=30.dp),color=TvColors.Muted,style=MaterialTheme.typography.labelLarge,maxLines=1)
            }
        }
    }
}
