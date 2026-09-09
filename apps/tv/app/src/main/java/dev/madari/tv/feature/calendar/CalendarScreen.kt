package dev.madari.tv.feature.calendar

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.madari.tv.core.Title
import dev.madari.tv.core.objects
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint

@Composable
fun CalendarScreen(state: TvState, vm: TvViewModel) {
    var month by rememberSaveable { mutableStateOf(java.time.YearMonth.now().toString()) }
    val selected = java.time.YearMonth.parse(month)
    val entries = state.calendar.optJSONArray("titles").objects().flatMap { entry ->
        val title = Title(entry.getJSONObject("key").text("installation_id"),entry.getJSONObject("meta"))
        title.videos.filter { it.text("released").startsWith(month) }.map { title to it }
    }.sortedBy { it.second.text("released") }
    LazyColumn(Modifier.fillMaxSize().padding(28.dp),verticalArrangement=Arrangement.spacedBy(18.dp)) {
        item { Heading("Calendar") }
        item { Row(horizontalArrangement=Arrangement.spacedBy(16.dp),verticalAlignment=Alignment.CenterVertically) {
            Action("Previous month",{month=selected.minusMonths(1).toString()})
            Hint(selected.format(java.time.format.DateTimeFormatter.ofPattern("MMMM yyyy")))
            Action("Next month",{month=selected.plusMonths(1).toString()})
            Action("Refresh",{vm.selectTab("Calendar")})
        } }
        if(!state.calendar.optBoolean("supported")) item { Hint("Install an addon with calendar support, then save series to My list.") }
        else if(entries.isEmpty()) item { Hint("No releases for your saved titles this month.") }
        items(entries,key={it.first.identity+it.second.text("id")}) { (title,episode) ->
            Action("${episode.text("released").take(10)}  ·  ${title.name}  ·  S${episode.optInt("season")} E${episode.optInt("episode")}",{vm.open(title)},Modifier.fillMaxWidth())
        }
    }
}
