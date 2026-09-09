package dev.madari.tv.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.tv.material3.Border
import androidx.tv.material3.Card
import androidx.tv.material3.CardDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import coil.compose.AsyncImage
import dev.madari.tv.core.Title
import dev.madari.tv.core.strings
import dev.madari.tv.core.text
import dev.madari.tv.ui.theme.TvColors

/**
 * Home hero card. Requests its full bounds when focused so the artwork and
 * title stay visible above the action row.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun Hero(
    title: Title,
    onOpen: () -> Unit,
    modifier: Modifier = Modifier,
    action: String = "More info",
    secondary: (@Composable () -> Unit)? = null,
    autoFocus: Boolean = false
) {
    val bounds=remember { BringIntoViewRequester() }
    val first=remember { FocusRequester() }
    var focused by remember { mutableStateOf(false) }
    LaunchedEffect(focused) { if(focused) bounds.bringIntoView() }
    Column(modifier.bringIntoViewRequester(bounds).padding(start=32.dp,end=32.dp,top=24.dp,bottom=8.dp)) {
        Card(onClick=onOpen,modifier=Modifier.fillMaxWidth().height(290.dp).focusRequester(first).onFocusChanged { focused=it.isFocused },
            scale=CardDefaults.scale(focusedScale=1f),shape=CardDefaults.shape(shape=RoundedCornerShape(10.dp)),
            border=CardDefaults.border(focusedBorder=Border(BorderStroke(1.dp,Color.White.copy(.65f)),shape=RoundedCornerShape(10.dp)))) {
            Box(Modifier.fillMaxSize().background(TvColors.Panel)) {
                AsyncImage(title.background,null,Modifier.fillMaxSize(),contentScale=ContentScale.Crop,alignment=Alignment.CenterEnd)
                Box(Modifier.fillMaxSize().background(Brush.verticalGradient(0f to Color.Transparent,.38f to Color.Transparent,1f to Color.Black.copy(.92f))))
                Box(Modifier.fillMaxSize().background(Brush.horizontalGradient(listOf(Color.Black.copy(.35f),Color.Transparent))))
                Column(Modifier.align(Alignment.BottomStart).padding(26.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
                    Text(if(title.type=="series") "SERIES" else "MOVIE",style=MaterialTheme.typography.labelMedium.copy(letterSpacing=3.sp),color=Color.White.copy(.8f))
                    TitleWordmark(title,Modifier.widthIn(max=430.dp).heightIn(max=96.dp))
                    Text(listOf(title.raw.text("releaseInfo"),title.raw.strings("genres").take(2).joinToString(" · ")).filter { it.isNotBlank() }.joinToString("   •   "),style=MaterialTheme.typography.bodyMedium,color=Color.White)
                }
                if(focused) Row(Modifier.align(Alignment.BottomEnd).padding(26.dp),verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(8.dp)) { Glyph("info",color=Color.White); Text("Explore title",style=MaterialTheme.typography.labelLarge,color=Color.White) }
            }
        }
    }
    LaunchedEffect(title.identity) { if(autoFocus) first.requestFocus() }
}

@Composable
fun TitleWordmark(title: Title, modifier: Modifier = Modifier) {
    val logo=title.raw.text("logo")
    var loaded by remember(logo) { mutableStateOf(false) }
    Box(modifier) {
        if(!loaded) Text(title.name,style=MaterialTheme.typography.displayMedium.copy(fontSize=40.sp,lineHeight=44.sp,fontWeight=FontWeight.Bold),color=Color.White,maxLines=2,overflow=TextOverflow.Ellipsis)
        if(logo.isNotBlank()) AsyncImage(logo,null,Modifier.width(340.dp).height(86.dp),contentScale=ContentScale.Fit,alignment=Alignment.CenterStart,onSuccess={loaded=true},onError={loaded=false})
    }
}
