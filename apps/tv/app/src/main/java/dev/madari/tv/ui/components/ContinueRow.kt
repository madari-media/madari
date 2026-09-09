package dev.madari.tv.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Card
import androidx.tv.material3.CardDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import coil.compose.AsyncImage
import dev.madari.tv.core.objects
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.state.continueTitles
import dev.madari.tv.state.sameKey
import dev.madari.tv.ui.theme.TvColors

/** Continue watching row driven by native progress and next-episode policy. */
@Composable
fun ContinueRow(state: TvState, vm: TvViewModel) {
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
