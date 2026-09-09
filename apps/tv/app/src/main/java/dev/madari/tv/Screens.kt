package dev.madari.tv

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp
import androidx.tv.material3.*
import coil.compose.AsyncImage
import org.json.JSONObject

@Composable fun HomeScreen(state: TvState, vm: TvViewModel) {
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
@Composable fun LibraryScreen(state: TvState, vm: TvViewModel) {
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
@Composable fun SearchScreen(state: TvState, vm: TvViewModel) {
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
@Composable fun ExploreScreen(state: TvState, vm: TvViewModel) {
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
@Composable fun SourcesScreen(state: TvState, vm: TvViewModel) {
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
            Text(title.name,style=MaterialTheme.typography.titleMedium,color=TvColors.Muted,maxLines=1,overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
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
                                border=CardDefaults.border(focusedBorder=Border(androidx.compose.foundation.BorderStroke(1.dp,androidx.compose.ui.graphics.Color.White),shape=androidx.compose.foundation.shape.RoundedCornerShape(8.dp)))) {
                                Row(Modifier.padding(20.dp),horizontalArrangement=Arrangement.spacedBy(18.dp),verticalAlignment=Alignment.CenterVertically) {
                                    Glyph("play",Modifier.size(24.dp))
                                    Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(7.dp)) {
                                        Text(source.raw.text("name").ifEmpty { source.name }.replace("\n"," · "),style=MaterialTheme.typography.titleMedium,maxLines=1,overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                                        val description=source.raw.text("description").ifEmpty { source.raw.text("title") }.replace("\n"," · ")
                                        if(description.isNotBlank()) Text(description,style=MaterialTheme.typography.bodyMedium,color=TvColors.Muted,maxLines=3,overflow=androidx.compose.ui.text.style.TextOverflow.Ellipsis)
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
@Composable fun SettingsScreen(state: TvState, vm: TvViewModel) {
    var pin by remember { mutableStateOf("") }
    var manifest by rememberSaveable { mutableStateOf("") }
    var local by rememberSaveable { mutableStateOf(false) }
    var profileName by rememberSaveable { mutableStateOf("") }
    var newPin by remember { mutableStateOf("") }
    var kids by rememberSaveable { mutableStateOf(false) }
    var removing by remember { mutableStateOf<String?>(null) }
    LazyColumn(Modifier.fillMaxSize().padding(32.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
        item { Heading("Settings") }
        item { WebSettingsCard(state, vm) }
        item { Hint("${state.profile?.text("name")} · On this TV") }
        item { Input(if(state.profile?.optBoolean("kids")==true) "Guardian PIN" else "Profile PIN (leave empty if none)",pin,{pin=it},Modifier.width(400.dp),secret=true) }
        item { Action("Switch profile",{vm.leave(pin)},enabled=!state.loading) }
        if(!state.settingsUnlocked) item { Action("Unlock settings",{vm.authorize(pin)},enabled=!state.loading) }
        else {
            item { Heading("Your addons") }
            items(state.snapshot.optJSONArray("addons").objects(),key={it.text("installation_id")}) { addon ->
                Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp),verticalArrangement=Arrangement.spacedBy(14.dp)) {
                    Text(addon.getJSONObject("manifest").text("name"),style=MaterialTheme.typography.titleLarge)
                    Row(horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                        Action(if(addon.optBoolean("enabled")) "Disable" else "Enable",{vm.enable(addon)})
                        Action("Move up",{vm.moveAddon(addon,-1)})
                        Action("Move down",{vm.moveAddon(addon,1)})
                        Action("Remove",{removing=addon.text("installation_id")})
                    }
                    if(removing==addon.text("installation_id")) Row(horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                        Action("Confirm removal",{vm.removeAddon(addon);removing=null})
                        Action("Cancel",{removing=null})
                    }
                }
            }
            item { Input("Addon manifest URL",manifest,{manifest=it}) }
            item { Action(if(local) "✓ Allow local-network addon" else "Allow local-network addon",{local=!local}) }
            item { Action("Install addon",{vm.install(manifest,local)},enabled=manifest.isNotBlank() && !state.loading) }
            item { Heading("Add a profile") }
            item { Input("Name",profileName,{profileName=it},Modifier.width(400.dp)) }
            item { Input("New profile PIN (optional)",newPin,{newPin=it},Modifier.width(400.dp),secret=true) }
            item { Action(if(kids) "✓ Kids profile" else "Kids profile",{kids=!kids}) }
            item { Action("Create profile",{vm.createProfile(profileName,newPin,kids)},enabled=profileName.isNotBlank() && !state.loading) }
            item { Hint("Kids profiles use adult-chosen addons. A guardian PIN protects settings and leaving kids mode.") }
        }
        item { Hint("Madari TV 0.1.0 · Kotlin / Compose for TV\nPowered by the same Madari core as the Linux app.") }
    }
}

@Composable fun CalendarScreen(state: TvState, vm: TvViewModel) {
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

@Composable fun WebSettingsCard(state: TvState, vm: TvViewModel) {
    Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
        Text("Manage from your phone or laptop",style=MaterialTheme.typography.titleLarge)
        if(state.web.optBoolean("running")) {
            Text(state.webAddress,color=TvColors.Accent,style=MaterialTheme.typography.headlineSmall)
            Text("Pairing code: ${state.web.text("code")}",style=MaterialTheme.typography.titleLarge)
            Hint("Open this address on the same network. Keep Madari open on the TV.")
        } else Hint("Start web settings to manage profiles, addon URLs and playback preferences in a browser.")
        Action(if(state.web.optBoolean("running")) "Stop web settings" else "Start web settings",vm::toggleWeb)
    }
}
