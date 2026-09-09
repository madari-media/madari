package dev.madari.tv.feature.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.unit.dp
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.state.savedTitles
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.components.Poster

@Composable
fun LibraryScreen(state: TvState, vm: TvViewModel) {
    val titles = savedTitles(state.snapshot)
    Column(Modifier.fillMaxSize().padding(28.dp),verticalArrangement=Arrangement.spacedBy(24.dp)) {
        Heading("My list")
        Hint("The stories you're saving for later")
        if(titles.isEmpty()) Hint("Explore a title and select Add to my list.")
        else LazyVerticalGrid(GridCells.Adaptive(142.dp),Modifier.fillMaxSize().focusRestorer(),contentPadding=PaddingValues(10.dp),horizontalArrangement=Arrangement.spacedBy(20.dp),verticalArrangement=Arrangement.spacedBy(24.dp)) {
            items(titles,key={it.identity}) { Poster(it,{vm.open(it)}) }
        }
    }
}
