package dev.madari.tv.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Border
import androidx.tv.material3.Card
import androidx.tv.material3.CardDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import coil.compose.AsyncImage
import dev.madari.tv.core.Shelf
import dev.madari.tv.core.Title
import dev.madari.tv.ui.theme.TvColors

/** Remembers the last focused card so returning to a row restores its position. */
val LocalCardFocus = staticCompositionLocalOf<MutableState<String?>?> { null }

@Composable
fun Poster(
    title: Title,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    onFocus: () -> Unit = {},
    focusKey: String = title.identity
) {
    val restore = LocalCardFocus.current
    val requester = remember { FocusRequester() }
    var focused by remember { mutableStateOf(false) }
    LaunchedEffect(focusKey) { if (restore?.value == focusKey) requester.requestFocus() }
    Card(
        onClick = onClick,
        modifier = modifier.width(128.dp).focusRequester(requester).onFocusChanged {
            focused = it.isFocused
            if (it.isFocused) { restore?.value = focusKey; onFocus() }
        }.semantics { contentDescription = title.name },
        scale = CardDefaults.scale(focusedScale = 1.025f),
        colors = CardDefaults.colors(containerColor = TvColors.Panel),
        border = CardDefaults.border(focusedBorder = Border(BorderStroke(1.dp, Color.White.copy(.65f)), shape = RoundedCornerShape(8.dp)))
    ) {
        Box(Modifier.height(188.dp).fillMaxWidth()) {
            Text(title.name, Modifier.align(Alignment.Center).padding(12.dp), style = MaterialTheme.typography.titleMedium, maxLines = 4)
            AsyncImage(title.poster, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
            if (focused) Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color.Transparent, Color.Transparent, Color.Black.copy(.9f))))) {
                Text(title.name, Modifier.align(Alignment.BottomStart).padding(10.dp), style = MaterialTheme.typography.bodyMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

@Composable
fun PosterRow(shelf: Shelf, onOpen: (Title) -> Unit, modifier: Modifier = Modifier, onMore: (() -> Unit)? = null) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(shelf.name + when (shelf.catalog?.type) { "movie" -> " · Movies"; "series" -> " · Series"; else -> "" }, Modifier.padding(horizontal = 32.dp), style = MaterialTheme.typography.titleMedium)
        LazyRow(Modifier.fillMaxWidth().focusRestorer(), contentPadding = PaddingValues(horizontal = 32.dp, vertical = 14.dp), horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            items(shelf.titles, key = { it.identity }, contentType = { "poster" }) { title -> Poster(title, { onOpen(title) }, focusKey = "${shelf.id}:${title.identity}") }
            if (shelf.more && onMore != null) item { Action("More", onMore) }
        }
    }
}
