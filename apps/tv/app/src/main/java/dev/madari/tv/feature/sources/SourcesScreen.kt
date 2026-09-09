package dev.madari.tv.feature.sources

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Card
import androidx.tv.material3.CardDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Glyph
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.theme.TvColors

@Composable
fun SourcesScreen(state: TvState, vm: TvViewModel) {
    val title=state.detail ?: return
    val sources=state.sources.orEmpty()
    var addon by rememberSaveable(title.identity,state.videoId) { mutableStateOf<String?>(null) }
    val groups=remember(sources) { sources.groupBy { it.provider } }
    val visible=if(addon==null) sources else sources.filter { it.provider==addon }
    val first=remember { FocusRequester() }
    LaunchedEffect(title.identity,state.videoId) { first.requestFocus() }
    Column(Modifier.fillMaxSize().padding(horizontal=36.dp,vertical=28.dp),verticalArrangement=Arrangement.spacedBy(22.dp)) {
        Column(verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Heading("Choose a source")
            Text(title.name,style=MaterialTheme.typography.titleMedium,color=TvColors.Muted,maxLines=1,overflow=TextOverflow.Ellipsis)
        }
        Row(Modifier.weight(1f),horizontalArrangement=Arrangement.spacedBy(28.dp)) {
            LazyColumn(Modifier.width(210.dp),contentPadding=PaddingValues(vertical=8.dp),verticalArrangement=Arrangement.spacedBy(10.dp)) {
                item { Text("ADDONS",Modifier.padding(bottom=8.dp),style=MaterialTheme.typography.labelMedium,color=TvColors.Muted) }
                item { Action("All addons  ·  ${sources.size}",{addon=null},Modifier.fillMaxWidth().focusRequester(first),primary=addon==null) }
                items(groups.keys.toList(),key={it}) { provider ->
                    val entries=groups.getValue(provider)
                    Action("${entries.first().name}  ·  ${entries.size}",{addon=provider},Modifier.fillMaxWidth(),primary=addon==provider)
                }
            }
            Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(12.dp)) {
                Text(if(state.loading) "Finding sources…" else "${visible.size} sources available",style=MaterialTheme.typography.labelLarge,color=TvColors.Muted)
                key(addon) {
                    LazyColumn(Modifier.fillMaxSize(),contentPadding=PaddingValues(6.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
                        if(visible.isEmpty()) item {
                            Column(Modifier.fillMaxWidth().padding(vertical=36.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
                                Text(if(state.loading) "Checking your addons" else "No sources found",style=MaterialTheme.typography.titleLarge)
                                Hint(if(state.loading) "Available streams will appear here." else "Try another addon or search again.")
                                if(!state.loading) Action("Try again",{vm.sources(title,state.videoId ?: title.id)})
                            }
                        }
                        items(visible.withIndex().toList(),key={it.index}) { (_,source) ->
                            Card(onClick={if(!state.loading) vm.play(title,state.videoId ?: title.id,source)},modifier=Modifier.fillMaxWidth(),
                                scale=CardDefaults.scale(focusedScale=1f),
                                shape=CardDefaults.shape(shape=androidx.compose.foundation.shape.RoundedCornerShape(8.dp)),
                                colors=CardDefaults.colors(containerColor=TvColors.Panel),
                                border=CardDefaults.border(focusedBorder=androidx.tv.material3.Border(androidx.compose.foundation.BorderStroke(1.dp,androidx.compose.ui.graphics.Color.White),shape=androidx.compose.foundation.shape.RoundedCornerShape(8.dp)))) {
                                Row(Modifier.padding(20.dp),horizontalArrangement=Arrangement.spacedBy(18.dp),verticalAlignment=Alignment.CenterVertically) {
                                    Glyph("play",Modifier.size(24.dp))
                                    Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(7.dp)) {
                                        Text(source.raw.text("name").ifEmpty { source.name }.replace("\n"," · "),style=MaterialTheme.typography.titleMedium,maxLines=1,overflow=TextOverflow.Ellipsis)
                                        val description=source.raw.text("description").ifEmpty { source.raw.text("title") }.replace("\n"," · ")
                                        if(description.isNotBlank()) Text(description,style=MaterialTheme.typography.bodyMedium,color=TvColors.Muted,maxLines=3,overflow=TextOverflow.Ellipsis)
                                        Text(source.name,style=MaterialTheme.typography.labelMedium,color=TvColors.Muted)
                                    }
                                }
                            }
                        }
                        items(state.notices) { Hint(it) }
                    }
                }
            }
        }
    }
}
