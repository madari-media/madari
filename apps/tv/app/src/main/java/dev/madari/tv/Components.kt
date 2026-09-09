package dev.madari.tv

import androidx.compose.animation.core.*
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.core.graphics.drawable.toBitmap
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.*
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.focus.*
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.*
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.*
import androidx.tv.material3.*
import coil.compose.AsyncImage

@Composable fun Glyph(name: String, modifier: Modifier = Modifier, color: Color = LocalContentColor.current) {
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
    Canvas(modifier.size(22.dp)) { scale(size.width/24f,size.height/24f,pivot=androidx.compose.ui.geometry.Offset.Zero) { drawPath(path,color,style=Stroke(1.7f,cap=StrokeCap.Round,join=StrokeJoin.Round)) } }
}
@Composable fun Action(label: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, icon: String? = null, primary: Boolean = false) {
    Button(onClick=onClick,modifier=modifier,enabled=enabled,
        shape=ButtonDefaults.shape(shape=RoundedCornerShape(6.dp)),scale=ButtonDefaults.scale(focusedScale=1.04f),
        colors=ButtonDefaults.colors(containerColor=if(primary) Color.White else TvColors.Panel,
            contentColor=if(primary) TvColors.Background else Color.White,focusedContainerColor=Color.White,focusedContentColor=TvColors.Background)) {
        if(icon!=null) { Glyph(icon); Spacer(Modifier.width(8.dp)) }
        Text(label,style=MaterialTheme.typography.labelLarge)
    }
}
@Composable fun Heading(text: String, modifier: Modifier = Modifier) { Text(text,modifier,style=MaterialTheme.typography.headlineMedium) }
@Composable fun Hint(text: String, modifier: Modifier = Modifier) { Text(text,modifier,color=TvColors.Muted,style=MaterialTheme.typography.bodyLarge) }
@Composable fun Input(label: String, value: String, onChange: (String) -> Unit, modifier: Modifier = Modifier, secret: Boolean = false) {
    var focused by remember { mutableStateOf(false) }
    Column(modifier,verticalArrangement=Arrangement.spacedBy(8.dp)) {
        Text(label,style=MaterialTheme.typography.bodyLarge)
        BasicTextField(value,onChange,singleLine=true,textStyle=TextStyle(color=Color.White,fontSize=18.sp),
            cursorBrush=SolidColor(TvColors.Accent),keyboardOptions=KeyboardOptions(keyboardType=if(secret) KeyboardType.NumberPassword else KeyboardType.Text),
            visualTransformation=if(secret) PasswordVisualTransformation() else VisualTransformation.None,
            modifier=Modifier.fillMaxWidth().onFocusChanged { focused=it.isFocused }.semantics { contentDescription=label }
                .background(TvColors.Panel,RoundedCornerShape(6.dp)).border(if(focused) 2.dp else 1.dp,if(focused) Color.White else Color.White.copy(.15f),RoundedCornerShape(6.dp)).padding(14.dp))
    }
}
val LocalCardFocus = staticCompositionLocalOf<MutableState<String?>?> { null }
@Composable fun Poster(title: Title, onClick: () -> Unit, modifier: Modifier = Modifier, onFocus: () -> Unit = {}, focusKey: String = title.identity) {
    val restore=LocalCardFocus.current
    val requester=remember { FocusRequester() }
    var focused by remember { mutableStateOf(false) }
    LaunchedEffect(focusKey) { if(restore?.value==focusKey) requester.requestFocus() }
    Card(onClick=onClick,modifier=modifier.width(128.dp).focusRequester(requester).onFocusChanged {
        focused=it.isFocused
        if(it.isFocused) { restore?.value=focusKey; onFocus() }
    }.semantics { contentDescription=title.name },
        scale=CardDefaults.scale(focusedScale=1.025f),colors=CardDefaults.colors(containerColor=TvColors.Panel),
        border=CardDefaults.border(focusedBorder=Border(BorderStroke(1.dp,Color.White.copy(.65f)),shape=RoundedCornerShape(8.dp)))) {
        Box(Modifier.height(188.dp).fillMaxWidth()) {
            Text(title.name,Modifier.align(Alignment.Center).padding(12.dp),style=MaterialTheme.typography.titleMedium,maxLines=4)
            AsyncImage(title.poster,null,Modifier.fillMaxSize(),contentScale=ContentScale.Crop)
            if(focused) Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Transparent,Color.Transparent,Color.Black.copy(.9f))))) {
                Text(title.name,Modifier.align(Alignment.BottomStart).padding(10.dp),style=MaterialTheme.typography.bodyMedium,maxLines=2,overflow=TextOverflow.Ellipsis)
            }
        }
    }
}
@Composable fun PosterRow(shelf: Shelf, onOpen: (Title) -> Unit, modifier: Modifier = Modifier, onMore: (() -> Unit)? = null) {
    Column(modifier,verticalArrangement=Arrangement.spacedBy(4.dp)) {
        Text(shelf.name + when(shelf.catalog?.type) { "movie" -> " · Movies"; "series" -> " · Series"; else -> "" },Modifier.padding(horizontal=32.dp),style=MaterialTheme.typography.titleMedium)
        LazyRow(Modifier.fillMaxWidth().focusRestorer(),contentPadding=PaddingValues(horizontal=32.dp,vertical=14.dp),horizontalArrangement=Arrangement.spacedBy(14.dp)) {
            items(shelf.titles,key={it.identity},contentType={"poster"}) { title -> Poster(title,{onOpen(title)},focusKey="${shelf.id}:${title.identity}") }
            if(shelf.more && onMore!=null) item { Action("More",onMore) }
        }
    }
}
@Composable fun ContinueRow(state: TvState, vm: TvViewModel) {
    val titles=continueTitles(state.snapshot)
    if(titles.isEmpty()) return
    Column {
        Text("Continue watching",Modifier.padding(horizontal=32.dp),style=MaterialTheme.typography.titleMedium)
        LazyRow(Modifier.fillMaxWidth().focusRestorer(),contentPadding=PaddingValues(horizontal=32.dp,vertical=14.dp),horizontalArrangement=Arrangement.spacedBy(16.dp)) {
            items(titles,key={it.identity}) { title ->
                val episode by produceState<org.json.JSONObject?>(null,title.identity,state.snapshot) {
                    try { value=vm.continueVideo(title) }
                    catch(e: kotlinx.coroutines.CancellationException) { throw e }
                    catch(_: Exception) { value=null }
                }
                if(title.type=="series" && title.videos.isNotEmpty() && episode==null) return@items
                val history=state.snapshot.optJSONArray("progress").objects().filter { sameKey(it.optJSONObject("key"),title.key) }
                val progress=if(episode!=null) history.lastOrNull { it.text("video_id")==episode!!.text("id") } else history.lastOrNull()
                val remaining=((progress?.optLong("duration_ms") ?: 0)-(progress?.optLong("position_ms") ?: 0)).coerceAtLeast(0)/60000
                val status=if(progress!=null && !progress.optBoolean("completed")) "Resume · $remaining min left" else if(title.type=="series") "Play next episode" else "Play movie"
                val fraction=(progress?.optLong("position_ms")?.toFloat() ?: 0f)/(progress?.optLong("duration_ms")?.coerceAtLeast(1) ?: 1)
                Card(onClick={if(!state.loading) vm.resumeContinue(title)},modifier=Modifier.width(280.dp),colors=CardDefaults.colors(containerColor=TvColors.Panel),scale=CardDefaults.scale(focusedScale=1f)) {
                    Box(Modifier.fillMaxWidth().height(174.dp)) {
                        AsyncImage(episode?.text("thumbnail")?.takeIf { it.isNotBlank() } ?: title.background,null,Modifier.fillMaxSize(),contentScale=ContentScale.Crop)
                        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Transparent,Color.Black.copy(.85f)))))
                        Column(Modifier.align(Alignment.BottomStart).padding(14.dp),verticalArrangement=Arrangement.spacedBy(5.dp)) {
                            Text(title.name,style=MaterialTheme.typography.titleMedium,maxLines=1,overflow=TextOverflow.Ellipsis)
                            episode?.let { Text("S${it.optInt("season")} · E${it.optInt("episode")}  ${it.text("title")}",style=MaterialTheme.typography.labelMedium,maxLines=1,overflow=TextOverflow.Ellipsis) }
                            Text(status,style=MaterialTheme.typography.labelMedium,color=Color.White)
                        }
                    }
                    Box(Modifier.fillMaxWidth().height(3.dp).background(Color.White.copy(.18f))) { Box(Modifier.fillMaxWidth(fraction.coerceIn(0f,1f)).fillMaxHeight().background(TvColors.Accent)) }
                }
            }
        }
    }
}
@OptIn(ExperimentalFoundationApi::class)
@Composable fun Hero(title: Title, onOpen: () -> Unit, modifier: Modifier = Modifier, action: String = "More info", secondary: (@Composable () -> Unit)? = null, autoFocus: Boolean = false) {
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

@Composable fun TitleWordmark(title: Title, modifier: Modifier = Modifier) {
    val logo=title.raw.text("logo")
    var loaded by remember(logo) { mutableStateOf(false) }
    Box(modifier) {
        if(!loaded) Text(title.name,style=MaterialTheme.typography.displayMedium.copy(fontSize=40.sp,lineHeight=44.sp,fontWeight=FontWeight.Bold),color=Color.White,maxLines=2,overflow=TextOverflow.Ellipsis)
        if(logo.isNotBlank()) AsyncImage(logo,null,Modifier.width(340.dp).height(86.dp),contentScale=ContentScale.Fit,alignment=Alignment.CenterStart,onSuccess={loaded=true},onError={loaded=false})
    }
}

@Composable fun BrandLogo(modifier: Modifier = Modifier) {
    Image(androidx.compose.ui.res.painterResource(R.drawable.madari_logo),"Madari",modifier.size(42.dp),contentScale=ContentScale.Fit)
}

@Composable fun HomeLoadingScreen(modifier: Modifier = Modifier) {
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


@OptIn(ExperimentalFoundationApi::class)
@Composable fun CinematicRow(shelf: Shelf, onOpen: (Title)->Unit, onMore: ()->Unit) {
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
                colors=CardDefaults.colors(containerColor=TvColors.Panel),border=CardDefaults.border(focusedBorder=Border(BorderStroke(1.dp,Color.White.copy(.8f)),shape=RoundedCornerShape(7.dp)))) {
                Box(Modifier.fillMaxSize()) {
                    PinnedArtwork(selected)
                    Box(Modifier.fillMaxSize().background(Brush.verticalGradient(
                        0f to Color.Transparent, .5f to Color.Transparent, 1f to Color.Black.copy(alpha=.82f)
                    )))
                    androidx.compose.animation.Crossfade(targetState=selected.name,modifier=Modifier.align(Alignment.BottomStart).fillMaxWidth().padding(18.dp),animationSpec=tween(260),label="card title") { name ->
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
