package dev.madari.tv.ui.navigation

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.madari.tv.ui.components.BrandLogo
import dev.madari.tv.ui.components.Glyph
import dev.madari.tv.ui.theme.TvColors

/** Fixed content inset: opening the rail never remeasures the catalog underneath. */
@Composable
fun NavigationRail(selected: String, profile: String, onSelect: (String)->Unit, first: FocusRequester) {
    var expanded by remember { mutableStateOf(false) }
    val focus=LocalFocusManager.current
    val entries=remember { listOf("Search" to "search","Home" to "home","Explore" to "explore","My list" to "plus","Calendar" to "calendar","Settings" to "settings") }
    Box(Modifier.width(if(expanded) 248.dp else 72.dp).fillMaxHeight()
        .background(Brush.horizontalGradient(listOf(TvColors.Background,TvColors.Background.copy(if(expanded) .98f else 1f),Color.Transparent)))) {
        Column(Modifier.width(if(expanded) 206.dp else 72.dp).fillMaxHeight().padding(vertical=26.dp)
            .onFocusChanged { expanded=it.hasFocus }.focusGroup(),horizontalAlignment=Alignment.Start) {
            BrandLogo(Modifier.padding(start=20.dp))
            Spacer(Modifier.weight(1f))
            entries.forEach { (tab,icon) ->
                var focused by remember { mutableStateOf(false) }
                Button(onClick={onSelect(tab);focus.moveFocus(FocusDirection.Right)},
                    modifier=Modifier.padding(start=14.dp,end=8.dp,bottom=9.dp).height(46.dp).fillMaxWidth()
                        .then(if(tab=="Home") Modifier.focusRequester(first) else Modifier)
                        .onFocusChanged { focused=it.isFocused }.semantics { contentDescription=tab },
                    contentPadding=PaddingValues(horizontal=14.dp),
                    shape=ButtonDefaults.shape(shape=RoundedCornerShape(8.dp)),
                    scale=ButtonDefaults.scale(focusedScale=1f),
                    colors=ButtonDefaults.colors(containerColor=Color.Transparent,contentColor=if(selected==tab) Color.White else TvColors.Muted,focusedContainerColor=Color.White.copy(.12f),focusedContentColor=Color.White)) {
                    Glyph(icon,Modifier.size(22.dp),if(focused || selected==tab) Color.White else TvColors.Muted)
                    if(expanded) { Spacer(Modifier.width(20.dp)); Text(tab,style=MaterialTheme.typography.titleMedium) }
                }
            }
            Spacer(Modifier.weight(1f))
            Text(if(expanded) profile else profile.take(1).uppercase(),Modifier.padding(start=28.dp),color=TvColors.Muted,style=MaterialTheme.typography.labelLarge,maxLines=1)
        }
    }
}
