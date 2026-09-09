package dev.madari.tv.feature.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.unit.dp
import dev.madari.tv.core.objects
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.state.continueTitles
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.CinematicRow
import dev.madari.tv.ui.components.ContinueRow
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hero
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.components.LocalCardFocus

@Composable
fun HomeScreen(state: TvState, vm: TvViewModel) {
    val candidate = state.shelves.firstNotNullOfOrNull { it.titles.firstOrNull() } ?: remember(state.snapshot) { continueTitles(state.snapshot).firstOrNull() }
    var hero by remember(state.profile?.text("id")) { mutableStateOf(candidate) }
    // Late catalog responses must not replace the hero or steal focus during browsing.
    LaunchedEffect(candidate?.identity) { if(hero==null) hero=candidate }
    LazyColumn(Modifier.fillMaxSize().focusRestorer(),contentPadding=PaddingValues(bottom=32.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
        if(hero != null) item("hero") { val featured=hero!!; Hero(featured,{vm.open(featured)},autoFocus=LocalCardFocus.current?.value==null) }
        else if(state.loading) item { Column(Modifier.height(302.dp).padding(42.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) { Heading("Your next story awaits"); Hint("Loading your catalogs…") } }
        else item { Column(Modifier.padding(36.dp),verticalArrangement=Arrangement.spacedBy(18.dp)) {
            Heading("A world of stories. Your way.")
            Hint(if(state.snapshot.optJSONArray("addons").objects().isEmpty()) "Install an addon to bring your movies and series to Madari." else "Your catalogs will appear here. Check enabled addons or try refreshing.")
            Action("Manage addons",{vm.selectTab("Settings")}); Action("Refresh",vm::refresh)
        } }
        val watching = continueTitles(state.snapshot)
        if(watching.isNotEmpty()) item("continue") { ContinueRow(state,vm) }
        items(state.shelves.filter{it.titles.isNotEmpty()},key={it.id}) { shelf -> CinematicRow(shelf,vm::open,onMore={vm.loadMore(shelf)}) }
        if(state.notices.isNotEmpty()) item { Column(Modifier.padding(24.dp)) { state.notices.forEach { Hint(it) }; Action("Retry catalogs",vm::refresh) } }
    }
}
