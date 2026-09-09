package dev.madari.tv

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.madari.tv.core.text
import dev.madari.tv.feature.calendar.CalendarScreen
import dev.madari.tv.feature.details.DetailsScreen
import dev.madari.tv.feature.explore.ExploreScreen
import dev.madari.tv.feature.home.HomeScreen
import dev.madari.tv.feature.library.LibraryScreen
import dev.madari.tv.feature.player.PlayerScreen
import dev.madari.tv.feature.profiles.ProfilesScreen
import dev.madari.tv.feature.search.SearchScreen
import dev.madari.tv.feature.settings.SettingsScreen
import dev.madari.tv.feature.sources.SourcesScreen
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Glyph
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.HomeLoadingScreen
import dev.madari.tv.ui.components.LocalCardFocus
import dev.madari.tv.ui.navigation.NavigationRail
import dev.madari.tv.ui.theme.MadariTheme
import dev.madari.tv.ui.theme.TvColors

class MainActivity : ComponentActivity() {
    var playerKeyHandler: ((android.view.KeyEvent) -> Boolean)? = null
    // Android Activity dispatch is public; core annotates its compatibility override as restricted.
    @Suppress("RestrictedApi")
    override fun dispatchKeyEvent(event: android.view.KeyEvent): Boolean = if(playerKeyHandler?.invoke(event)==true) true else super.dispatchKeyEvent(event)
    private val viewModel: TvViewModel by viewModels()
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { MadariTheme { TvApp(viewModel) } }
    }
}
@Composable fun TvApp(vm: TvViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    val first = remember { FocusRequester() }
    val savedScreens = rememberSaveableStateHolder()
    val homeLoading = state.loading && state.tab=="Home" && state.detail==null && state.shelves.all { it.titles.isEmpty() }
    BackHandler(state.playback == null && (state.resumingTitle != null || state.detail != null || state.catalog != null || state.tab != "Home")) { vm.back() }
    Box(Modifier.fillMaxSize().background(TvColors.Background)) {
        val playback = state.playback
        if(playback != null) key(playback.uri,playback.videoId) { PlayerScreen(playback,vm) }
        else if(state.profile == null) ProfilesScreen(state,vm)
        else androidx.compose.animation.Crossfade(
            targetState=homeLoading,
            animationSpec=androidx.compose.animation.core.tween(420),
            label="home loading transition"
        ) { loading ->
        if(loading) HomeLoadingScreen()
        else Box(Modifier.fillMaxSize()) {
            Box(Modifier.fillMaxSize().padding(start=72.dp)) {
                when {
                    state.sources != null -> SourcesScreen(state,vm)
                    state.detail != null -> DetailsScreen(state,vm)
                    else -> savedScreens.SaveableStateProvider(state.tab) {
                        val lastCard = rememberSaveable { mutableStateOf<String?>(null) }
                        CompositionLocalProvider(LocalCardFocus provides lastCard) {
                            when(state.tab) {
                                "Settings" -> SettingsScreen(state,vm)
                                "Search" -> SearchScreen(state,vm)
                                "Explore" -> ExploreScreen(state,vm)
                                "My list" -> LibraryScreen(state,vm)
                                "Calendar" -> CalendarScreen(state,vm)
                                else -> HomeScreen(state,vm)
                            }
                        }
                    }
                }
            }
            NavigationRail(state.tab,state.profile?.text("name").orEmpty(),vm::selectTab,first)

        }
        }
        androidx.compose.animation.AnimatedVisibility(
            visible=state.resumingTitle!=null && state.playback==null,
            modifier=Modifier.align(Alignment.BottomCenter).padding(bottom=28.dp),
            enter=androidx.compose.animation.fadeIn() + androidx.compose.animation.slideInVertically { it / 2 },
            exit=androidx.compose.animation.fadeOut()
        ) {
            Row(Modifier.background(TvColors.Panel,RoundedCornerShape(8.dp)).padding(horizontal=24.dp,vertical=16.dp)
                .semantics { liveRegion=androidx.compose.ui.semantics.LiveRegionMode.Polite },
                verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(14.dp)) {
                Glyph("play",Modifier.size(20.dp))
                Text("Resuming…",style=MaterialTheme.typography.bodyLarge)
                Text("Back to cancel",style=MaterialTheme.typography.labelMedium,color=TvColors.Muted)
            }
        }
        if(state.loading && !homeLoading && state.resumingTitle==null) Box(Modifier.align(Alignment.BottomEnd).padding(24.dp).background(TvColors.Panel).padding(horizontal=20.dp,vertical=12.dp)) {
            Text("Loading…",style=MaterialTheme.typography.bodyLarge)
        }
        state.error?.let { message ->
            Dialog(onDismissRequest=vm::dismissError) {
                Column(Modifier.width(520.dp).background(TvColors.Panel).padding(28.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
                    Heading("Something went wrong")
                    Text(message,style=MaterialTheme.typography.bodyLarge)
                    val close = remember { FocusRequester() }
                    Action("Dismiss",vm::dismissError,Modifier.focusRequester(close))
                    LaunchedEffect(Unit) { close.requestFocus() }
                }
            }
        }
    }
}
