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
import dev.madari.tv.state.ContinueEntry
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.state.continueTitles
import dev.madari.tv.state.sameKey
import dev.madari.tv.ui.theme.TvColors

/**
 * Continue watching row. Metadata is resolved once per snapshot with the cached
 * `continue_metadata` batch, then each series asks the core for its next video.
 * A series with episodes but no next video is finished and stays hidden.
 */
@Composable
fun ContinueRow(state: TvState, vm: TvViewModel) {
    val titles = continueTitles(state.snapshot)
    if (titles.isEmpty()) return
    val entries by produceState<List<ContinueEntry>?>(null, state.snapshot) {
        value = try { vm.continueEntries(titles) }
        catch (e: kotlinx.coroutines.CancellationException) { throw e }
        catch (_: Exception) { emptyList() }
    }
    val byId = entries?.associateBy { it.title.identity }
    val visible = titles.filter { title ->
        val entry = byId?.get(title.identity)
        !(title.type == "series" && entry != null && entry.hasVideos && entry.episode == null)
    }
    if (visible.isEmpty()) return
    Column {
        Text("Continue watching", Modifier.padding(horizontal = 32.dp), style = MaterialTheme.typography.titleMedium)
        LazyRow(Modifier.fillMaxWidth().focusRestorer(), contentPadding = PaddingValues(horizontal = 32.dp, vertical = 14.dp), horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            items(visible, key = { it.identity }) { title ->
                val entry = byId?.get(title.identity)
                val episode = entry?.episode
                val artwork = episode?.text("thumbnail")?.takeIf { it.isNotBlank() }
                    ?: entry?.meta?.text("background")?.takeIf { it.isNotBlank() }
                    ?: title.background
                val history = state.snapshot.optJSONArray("progress").objects().filter { sameKey(it.optJSONObject("key"), title.key) }
                // Match progress to the video the core selected, so an unwatched next
                // episode shows no bar instead of the previous episode's position.
                val target = episode?.text("id") ?: history.lastOrNull()?.text("video_id")
                val progress = history.lastOrNull { it.text("video_id") == target }
                val completed = progress?.optBoolean("completed") ?: false
                val remaining = ((progress?.optLong("duration_ms") ?: 0) - (progress?.optLong("position_ms") ?: 0)).coerceAtLeast(0) / 60000
                val status = when {
                    progress != null && !completed && progress.optLong("position_ms") > 0 -> "Resume · $remaining min left"
                    title.type == "series" && (episode != null || completed) -> "Play next episode"
                    title.type == "series" -> "Continue series"
                    else -> "Play movie"
                }
                val fraction = (progress?.optLong("position_ms")?.toFloat() ?: 0f) / (progress?.optLong("duration_ms")?.coerceAtLeast(1) ?: 1)
                Card(onClick = { vm.resumeContinue(title, episode?.text("id")) }, modifier = Modifier.width(280.dp), colors = CardDefaults.colors(containerColor = TvColors.Panel), scale = CardDefaults.scale(focusedScale = 1f)) {
                    Box(Modifier.fillMaxWidth().height(174.dp)) {
                        AsyncImage(artwork, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
                        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black.copy(.85f)))))
                        Column(Modifier.align(Alignment.BottomStart).padding(14.dp), verticalArrangement = Arrangement.spacedBy(5.dp)) {
                            Text(title.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            episode?.let { Text("S${it.optInt("season")} · E${it.optInt("episode")}  ${it.text("title")}", style = MaterialTheme.typography.labelMedium, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                            Text(status, style = MaterialTheme.typography.labelMedium, color = Color.White)
                        }
                    }
                    Box(Modifier.fillMaxWidth().height(3.dp).background(Color.White.copy(.18f))) { Box(Modifier.fillMaxWidth(fraction.coerceIn(0f, 1f)).fillMaxHeight().background(TvColors.Accent)) }
                }
            }
        }
    }
}
