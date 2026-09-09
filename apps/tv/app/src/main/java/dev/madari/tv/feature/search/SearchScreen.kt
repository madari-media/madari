package dev.madari.tv.feature.search

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
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
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.components.Input
import dev.madari.tv.ui.components.PosterRow

@Composable
fun SearchScreen(state: TvState, vm: TvViewModel) {
    var query by rememberSaveable { mutableStateOf(state.query) }
    Column(Modifier.fillMaxSize().padding(top=28.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
        Heading("Find your next favourite",Modifier.padding(horizontal=24.dp))
        Row(Modifier.padding(horizontal=24.dp),horizontalArrangement=Arrangement.spacedBy(16.dp),verticalAlignment=Alignment.Bottom) {
            Input("Search movies and series",query,{query=it},Modifier.weight(1f))
            Action("Search",{vm.search(query)},enabled=!state.loading && query.isNotBlank())
        }
        LazyColumn(Modifier.fillMaxSize()) {
            if(state.query.isEmpty()) item { Hint("Search across your enabled addons.",Modifier.padding(24.dp)) }
            else if(!state.loading && state.shelves.all{it.titles.isEmpty()}) item { Hint("No results. Try another title or check your searchable addons.",Modifier.padding(24.dp)) }
            items(state.shelves.filter{it.titles.isNotEmpty()},key={it.id}) { shelf -> PosterRow(shelf,vm::open,onMore={vm.loadMore(shelf)}) }
            items(state.notices) { Hint(it,Modifier.padding(24.dp)) }
        }
    }
}
