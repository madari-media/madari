package dev.madari.tv.feature.explore

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.unit.dp
import dev.madari.tv.core.Catalog
import dev.madari.tv.core.strings
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.components.Input
import dev.madari.tv.ui.components.Poster

@Composable
fun ExploreScreen(state: TvState, vm: TvViewModel) {
    var selected by remember { mutableStateOf<Catalog?>(null) }
    val fields = remember(selected?.identity) { mutableStateMapOf<String,String>() }
    val catalog = state.catalog
    if(catalog != null) Column(Modifier.fillMaxSize().padding(28.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
        Heading(catalog.name); Hint(catalog.providerName)
        val shelf = state.shelves.firstOrNull()
        if(shelf != null) LazyVerticalGrid(GridCells.Adaptive(142.dp),Modifier.fillMaxSize().focusRestorer(),contentPadding=PaddingValues(10.dp),horizontalArrangement=Arrangement.spacedBy(18.dp),verticalArrangement=Arrangement.spacedBy(24.dp)) {
            items(shelf.titles,key={it.identity}) { Poster(it,{vm.open(it)}) }
            if(shelf.more) item { Action("Load more",{vm.loadMore(shelf)}) }
            if(shelf.titles.isEmpty()) item { Hint("No titles found") }
        }
    } else LazyColumn(Modifier.fillMaxSize().padding(28.dp),verticalArrangement=Arrangement.spacedBy(18.dp)) {
        item { Heading("Explore") }
        item { Hint("Browse your catalogs") }
        if(vm.catalogs().isEmpty()) item { Action("Install an addon",{vm.selectTab("Settings")}) }
        items(vm.catalogs(),key={it.identity}) { entry ->
            Action("${entry.name}  ·  ${entry.type}  ·  ${entry.providerName}",{selected=entry},Modifier.fillMaxWidth())
            if(selected?.identity == entry.identity) Column(Modifier.padding(16.dp),verticalArrangement=Arrangement.spacedBy(14.dp)) {
                (entry.extras.filter{it.text("name") != "skip"}.map{it.text("name")} + entry.required).distinct().forEach { field ->
                    Input(field + if(field in entry.required) " (required)" else " (optional)",fields[field].orEmpty(),{fields[field]=it})
                    val options = entry.extras.firstOrNull{it.text("name")==field}?.strings("options").orEmpty()
                    if(options.isNotEmpty()) LazyRow(horizontalArrangement=Arrangement.spacedBy(10.dp),contentPadding=PaddingValues(8.dp)) { items(options) { option -> Action(option,{fields[field]=option}) } }
                }
                Action("Open catalog",{vm.openCatalog(entry,fields.filterValues{it.isNotBlank()}.toMap())},enabled=entry.required.all{!fields[it].isNullOrBlank()})
            }
        }
    }
}
