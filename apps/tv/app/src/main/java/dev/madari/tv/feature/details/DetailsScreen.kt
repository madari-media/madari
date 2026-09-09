package dev.madari.tv.feature.details

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.tv.material3.*
import coil.compose.AsyncImage
import dev.madari.tv.core.objects
import dev.madari.tv.core.strings
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.state.sameKey
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.components.TitleWordmark
import dev.madari.tv.ui.theme.TvColors

/** Title overview: a fixed viewport; only the episode and synopsis panels scroll. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun DetailsScreen(state: TvState, vm: TvViewModel) {
    val title=state.detail ?: return
    val videos=title.videos
    val seasons=remember(title) { videos.map { it.optInt("season") }.distinct() }
    val progress=remember(state.snapshot,title.identity) { state.snapshot.optJSONArray("progress").objects().filter { sameKey(it.optJSONObject("key"),title.key) } }
    val video=state.videoId ?: title.id
    val watching=progress.lastOrNull { it.text("video_id")==video && !it.optBoolean("completed") && it.optLong("position_ms")>0 }
    val current=videos.firstOrNull { it.text("id")==video }
    var season by rememberSaveable(title.identity) { mutableIntStateOf(current?.optInt("season") ?: seasons.firstOrNull() ?: 1) }
    LaunchedEffect(seasons) { if(season !in seasons) season=seasons.firstOrNull() ?: 1 }
    var episodesOpen by remember { mutableStateOf(false) }
    var entered by remember { mutableStateOf(false) }
    val entryAlpha by animateFloatAsState(if(entered) 1f else 0f,tween(220),label="detail entrance")
    LaunchedEffect(Unit) { entered=true }
    var about by remember { mutableStateOf(false) }
    val watchFocus=remember { FocusRequester() }
    val episodesFocus=remember { FocusRequester() }
    val infoFocus=remember { FocusRequester() }
    var initialFocusSet by remember(title.identity) { mutableStateOf(false) }
    var panelReturn by remember { mutableStateOf<FocusRequester?>(null) }
    LaunchedEffect(title.identity) {
        if(!initialFocusSet) { watchFocus.requestFocus(); initialFocusSet=true }
    }
    LaunchedEffect(episodesOpen,about) {
        if(!episodesOpen && !about) { panelReturn?.requestFocus(); panelReturn=null }
    }
    // A TV overview is a fixed viewport. Only the episode and synopsis panels scroll.
    Box(Modifier.fillMaxSize().background(TvColors.Background).graphicsLayer { alpha=entryAlpha }) {
        AsyncImage(title.background,null,Modifier.fillMaxSize(),contentScale=ContentScale.Crop,alignment=Alignment.CenterEnd)
        Box(Modifier.fillMaxSize().background(Brush.horizontalGradient(0f to TvColors.Background,.34f to TvColors.Background.copy(.90f),.70f to Color.Black.copy(.12f),1f to Color.Transparent)))
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(0f to Color.Black.copy(.1f),.65f to Color.Transparent,1f to TvColors.Background)))
        Column(Modifier.align(Alignment.CenterStart).padding(start=42.dp,top=28.dp,bottom=28.dp).width(470.dp),verticalArrangement=Arrangement.spacedBy(14.dp)) {
            Text(if(title.type=="series") "SERIES" else "MOVIE",color=TvColors.Muted,style=MaterialTheme.typography.labelLarge.copy(letterSpacing=3.sp))
            TitleWordmark(title,Modifier.width(420.dp).height(90.dp))
            val metadata=listOf(title.raw.text("releaseInfo"),title.raw.text("runtime"),title.raw.text("imdbRating").takeIf { it.isNotBlank() }?.let { "IMDb $it" }.orEmpty(),if(seasons.isNotEmpty()) "${seasons.size} season${if(seasons.size==1) "" else "s"}" else "").filter { it.isNotBlank() }
            if(metadata.isNotEmpty()) Text(metadata.joinToString("   ·   "),style=MaterialTheme.typography.bodyLarge,maxLines=1,overflow=TextOverflow.Ellipsis)
            Text(title.description.ifEmpty { title.raw.strings("genres").joinToString(" · ") },Modifier.width(440.dp),style=MaterialTheme.typography.bodyLarge.copy(lineHeight=23.sp),color=Color.White.copy(.8f),maxLines=3,overflow=TextOverflow.Ellipsis)
            val episodeLabel=current?.let { "Season ${it.optInt("season")} · Episode ${it.optInt("episode")}" }
            if(episodeLabel!=null) Text(episodeLabel,style=MaterialTheme.typography.labelLarge,color=TvColors.Muted)
            Column(Modifier.width(290.dp),verticalArrangement=Arrangement.spacedBy(10.dp)) {
                Action(if(state.loading) "Loading title…" else if(watching!=null) "Resume watching" else "Watch now",{if(!state.loading) vm.sources(title,video)},Modifier.fillMaxWidth().height(46.dp).focusRequester(watchFocus),icon="play",primary=true)
                if(videos.isNotEmpty()) Action("Episodes & seasons",{panelReturn=episodesFocus;episodesOpen=true},Modifier.fillMaxWidth().height(44.dp).focusRequester(episodesFocus),icon="episodes")
                Row(horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                    Action(if(vm.isSaved(title)) "Saved" else "My List",{vm.toggleSaved(title)},Modifier.weight(1f),icon=if(vm.isSaved(title)) "check" else "plus")
                    Action("Details",{panelReturn=infoFocus;about=true},Modifier.weight(1f).focusRequester(infoFocus),icon="info")
                }
            }
            if(watching!=null) {
                val duration=watching.optLong("duration_ms").coerceAtLeast(1)
                val position=watching.optLong("position_ms")
                Row(verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(14.dp)) {
                    Box(Modifier.width(160.dp).height(3.dp).background(Color.White.copy(.2f))) { Box(Modifier.fillMaxWidth((position.toFloat()/duration).coerceIn(0f,1f)).fillMaxHeight().background(TvColors.Accent)) }
                    Text("${((duration-position).coerceAtLeast(0)+59999)/60000} min left",color=TvColors.Muted,style=MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }
    if(episodesOpen) Dialog(onDismissRequest={episodesOpen=false},properties=androidx.compose.ui.window.DialogProperties(usePlatformDefaultWidth=false)) {
        val close=remember { FocusRequester() }
        val seasonScroll=rememberLazyListState(initialFirstVisibleItemIndex=seasons.indexOf(season).coerceAtLeast(0))
        val episodeScroll=rememberLazyListState()
        LaunchedEffect(season) { episodeScroll.scrollToItem(0) }
        Row(Modifier.fillMaxSize().background(TvColors.Background).padding(42.dp),horizontalArrangement=Arrangement.spacedBy(36.dp)) {
            Column(Modifier.width(230.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
                TitleWordmark(title,Modifier.fillMaxWidth().heightIn(max=100.dp))
                Text("Episodes",style=MaterialTheme.typography.headlineSmall)
                LazyColumn(Modifier.weight(1f),state=seasonScroll,verticalArrangement=Arrangement.spacedBy(10.dp)) {
                    items(seasons) { number -> Action(if(number==0) "Specials" else "Season $number",{season=number},Modifier.fillMaxWidth().then(if(number==season) Modifier.focusRequester(close) else Modifier),primary=season==number) }
                }
                Action("Back to title",{episodesOpen=false})
            }
            LazyColumn(Modifier.weight(1f).fillMaxHeight().focusRestorer(),state=episodeScroll,contentPadding=PaddingValues(8.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
                items(videos.filter { it.optInt("season")==season },key={it.text("id")}) { episode ->
                    val watched=progress.lastOrNull { it.text("video_id")==episode.text("id") }
                    Card(onClick={episodesOpen=false;vm.sources(title,episode.text("id"))},scale=CardDefaults.scale(focusedScale=1.015f),colors=CardDefaults.colors(containerColor=TvColors.Panel),shape=CardDefaults.shape(shape=RoundedCornerShape(8.dp))) {
                        Row(Modifier.fillMaxWidth().padding(12.dp),horizontalArrangement=Arrangement.spacedBy(18.dp),verticalAlignment=Alignment.CenterVertically) {
                            Box(Modifier.size(176.dp,100.dp)) {
                                AsyncImage(episode.text("thumbnail").ifEmpty { title.background },null,Modifier.fillMaxSize(),contentScale=ContentScale.Crop)
                                if(watched!=null) Box(Modifier.align(Alignment.BottomStart).fillMaxWidth().height(3.dp).background(Color.White.copy(.3f))) {
                                    Box(Modifier.fillMaxWidth(if(watched.optBoolean("completed")) 1f else (watched.optLong("position_ms").toFloat()/watched.optLong("duration_ms").coerceAtLeast(1)).coerceIn(0f,1f)).fillMaxHeight().background(TvColors.Accent))
                                }
                            }
                            Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                                Text("${episode.optInt("episode")}. ${episode.text("title").ifEmpty { episode.text("name").ifEmpty { "Episode ${episode.optInt("episode")}" } }}",style=MaterialTheme.typography.titleMedium,maxLines=2,overflow=TextOverflow.Ellipsis)
                                Text(episode.text("overview").ifEmpty { episode.text("description") },style=MaterialTheme.typography.bodyMedium,maxLines=3,overflow=TextOverflow.Ellipsis)
                                if(watched!=null) Text(if(watched.optBoolean("completed")) "Watched" else "Continue watching",style=MaterialTheme.typography.labelMedium)
                            }
                        }
                    }
                }
            }
        }
        LaunchedEffect(Unit) { close.requestFocus() }
    }
    if(about) Dialog(onDismissRequest={about=false}) {
        val close=remember { FocusRequester() }
        Column(Modifier.width(620.dp).heightIn(max=470.dp).background(TvColors.Panel,RoundedCornerShape(12.dp)).padding(30.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
            Heading(title.name)
            Column(Modifier.weight(1f,false).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(16.dp)) {
                Text(title.description.ifBlank { "No synopsis available." },style=MaterialTheme.typography.bodyLarge)
                for((label,key) in listOf("Cast" to "cast","Director" to "director","Genres" to "genres")) {
                    val values=title.raw.strings(key)
                    if(values.isNotEmpty()) { Text(label,style=MaterialTheme.typography.titleMedium); Hint(values.joinToString(" · ")) }
                }
            }
            Action("Close",{about=false},Modifier.focusRequester(close))
        }
        LaunchedEffect(Unit) { close.requestFocus() }
    }
}
