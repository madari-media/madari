package dev.madari.tv.ui.components

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.graphics.drawable.toBitmap
import androidx.tv.material3.Border
import androidx.tv.material3.Card
import androidx.tv.material3.CardDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import coil.compose.AsyncImage
import dev.madari.tv.core.Shelf
import dev.madari.tv.core.Title
import dev.madari.tv.ui.theme.TvColors

/**
 * Cinematic catalog row. One permanently wide focus slot stays at the leading
 * inset; D-pad left/right changes its content without moving or resizing it.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun CinematicRow(shelf: Shelf, onOpen: (Title)->Unit, onMore: ()->Unit) {
    if(shelf.titles.isEmpty()) return
    val restore=LocalCardFocus.current
    var selectedId by androidx.compose.runtime.saveable.rememberSaveable(shelf.id) { mutableStateOf(shelf.titles.first().identity) }
    val index=shelf.titles.indexOfFirst { it.identity==selectedId }.coerceAtLeast(0)
    val selected=shelf.titles[index]
    val focus=remember { FocusRequester() }
    val tail=androidx.compose.foundation.lazy.rememberLazyListState(initialFirstVisibleItemIndex=(index+1).coerceAtMost(shelf.titles.size))
    val context=androidx.compose.ui.platform.LocalContext.current
    val density=androidx.compose.ui.platform.LocalDensity.current
    val stride=with(density) { 140.dp.toPx() }
    val artworkWidth=with(density) { 336.dp.roundToPx() }
    val artworkHeight=with(density) { 189.dp.roundToPx() }
    var rowFocused by remember { mutableStateOf(false) }
    val rowBounds=remember { BringIntoViewRequester() }
    var consumedKey by remember { mutableIntStateOf(-1) }
    LaunchedEffect(index,stride) {
        withFrameNanos { }
        val distance=(index+1-tail.firstVisibleItemIndex)*stride-tail.firstVisibleItemScrollOffset
        if(kotlin.math.abs(distance)>1f) tail.animateScrollBy(distance,androidx.compose.animation.core.tween(280,easing=androidx.compose.animation.core.FastOutSlowInEasing))
    }
    LaunchedEffect(rowFocused) { if(rowFocused) rowBounds.bringIntoView() }
    DisposableEffect(index,rowFocused,artworkWidth,artworkHeight) {
        val jobs=if(rowFocused) shelf.titles.subList((index-1).coerceAtLeast(0),(index+4).coerceAtMost(shelf.titles.size)).map { title ->
            coil.Coil.imageLoader(context).enqueue(coil.request.ImageRequest.Builder(context).data(title.background).size(artworkWidth,artworkHeight).allowHardware(false).build())
        } else emptyList()
        onDispose { jobs.forEach { it.dispose() } }
    }
    LaunchedEffect(shelf.id) { if(restore?.value?.startsWith("${shelf.id}:")==true) focus.requestFocus() }
    Column(Modifier.bringIntoViewRequester(rowBounds).padding(vertical=14.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
        Text(shelf.name + when(shelf.catalog?.type) { "movie" -> " · Movies"; "series" -> " · Series"; else -> "" },Modifier.padding(horizontal=32.dp),style=MaterialTheme.typography.titleMedium)
        Row(Modifier.fillMaxWidth().padding(start=32.dp,top=12.dp,bottom=12.dp),horizontalArrangement=Arrangement.spacedBy(14.dp)) {
            // One permanent wide focus target. D-pad changes its content, never its bounds.
            Card(onClick={onOpen(selected)},modifier=Modifier.size(336.dp,189.dp).focusRequester(focus).onFocusChanged {
                rowFocused=it.isFocused
                if(it.isFocused) restore?.value="${shelf.id}:${selected.identity}"
            }.onPreviewKeyEvent { key ->
                val event=key.nativeKeyEvent
                val code=event.keyCode
                if(event.action==android.view.KeyEvent.ACTION_UP && consumedKey==code) { consumedKey=-1;true }
                else if(event.action==android.view.KeyEvent.ACTION_DOWN && (code==android.view.KeyEvent.KEYCODE_DPAD_RIGHT || (code==android.view.KeyEvent.KEYCODE_DPAD_LEFT && index>0))) {
                    consumedKey=code
                    val target=if(code==android.view.KeyEvent.KEYCODE_DPAD_RIGHT) index+1 else index-1
                    if(target in shelf.titles.indices) { selectedId=shelf.titles[target].identity;restore?.value="${shelf.id}:$selectedId" }
                    else if(shelf.more && event.repeatCount==0) onMore()
                    true
                } else false
            }.semantics { contentDescription=selected.name },scale=CardDefaults.scale(focusedScale=1f),shape=CardDefaults.shape(shape=RoundedCornerShape(7.dp)),
                colors=CardDefaults.colors(containerColor=TvColors.Panel),border=CardDefaults.border(focusedBorder=Border(androidx.compose.foundation.BorderStroke(1.dp,Color.White.copy(.8f)),shape=RoundedCornerShape(7.dp)))) {
                Box(Modifier.fillMaxSize()) {
                    PinnedArtwork(selected)
                    Box(Modifier.fillMaxSize().background(Brush.verticalGradient(
                        0f to Color.Transparent, .5f to Color.Transparent, 1f to Color.Black.copy(alpha=.82f)
                    )))
                    androidx.compose.animation.Crossfade(targetState=selected.name,modifier=Modifier.align(Alignment.BottomStart).fillMaxWidth().padding(18.dp),animationSpec=androidx.compose.animation.core.tween(260),label="card title") { name ->
                        Text(name,style=MaterialTheme.typography.titleLarge,color=Color.White,maxLines=2,overflow=TextOverflow.Ellipsis)
                    }
                }
            }
            BoxWithConstraints(Modifier.weight(1f).height(189.dp)) {
                val trailingWidth=maxWidth
                LazyRow(Modifier.fillMaxSize(),state=tail,userScrollEnabled=false,horizontalArrangement=Arrangement.spacedBy(14.dp)) {
                    items(shelf.titles,key={it.identity}) { title ->
                        // Narrow neighbours are previews, not additional wide focus targets.
                        AsyncImage(title.poster,null,Modifier.size(126.dp,189.dp).clip(RoundedCornerShape(7.dp)).background(TvColors.Panel),contentScale=ContentScale.Crop)
                    }
                    item("end") { Spacer(Modifier.width(trailingWidth).height(189.dp)) }
                }
            }
        }

    }
}

@Composable private fun PinnedArtwork(title: Title) {
    val context=androidx.compose.ui.platform.LocalContext.current
    val density=androidx.compose.ui.platform.LocalDensity.current
    val width=with(density) { 336.dp.roundToPx() }
    val height=with(density) { 189.dp.roundToPx() }
    var ready by remember { mutableStateOf<ImageBitmap?>(null) }
    LaunchedEffect(title.background,title.poster,width,height) {
        // Keep the previous image until a replacement is decoded. Cancel obsolete requests.
        val loaded=kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
            val loader=coil.Coil.imageLoader(context)
            var result=loader.execute(coil.request.ImageRequest.Builder(context).data(title.background).size(width,height).allowHardware(false).build())
            if(result !is coil.request.SuccessResult && title.poster!=title.background) result=loader.execute(coil.request.ImageRequest.Builder(context).data(title.poster).size(width,height).allowHardware(false).build())
            (result as? coil.request.SuccessResult)?.drawable?.toBitmap()?.asImageBitmap()
        }
        ready=loaded
    }
    androidx.compose.animation.Crossfade(targetState=ready,modifier=Modifier.fillMaxSize(),animationSpec=androidx.compose.animation.core.tween(280),label="pinned artwork") { bitmap ->
        if(bitmap!=null) Image(bitmap,null,Modifier.fillMaxSize(),contentScale=ContentScale.Crop)
        else Box(Modifier.fillMaxSize().background(TvColors.Panel))
    }
}
