package dev.madari.tv.feature.profiles

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.tv.material3.*
import coil.compose.AsyncImage
import dev.madari.tv.core.text
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Glyph
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.theme.TvColors
import org.json.JSONObject

/** Initials remain visible while loading or offline; only successful artwork replaces them. */
@Composable
internal fun AvatarArtwork(name: String, url: String?, modifier: Modifier = Modifier) {
    var loaded by remember(url) { mutableStateOf(false) }
    Box(modifier, contentAlignment = Alignment.Center) {
        if (!loaded) Text(name.trim().take(1).uppercase().ifEmpty { "?" }, color = Color.White, fontSize = 36.sp)
        if (!url.isNullOrEmpty()) AsyncImage(
            model = url,
            contentDescription = null,
            modifier = Modifier.fillMaxSize(),
            contentScale = ContentScale.Crop,
            onSuccess = { loaded = true },
            onError = { loaded = false }
        )
    }
}

@Composable
internal fun AvatarField(
    avatar: String,
    avatars: List<JSONObject>,
    name: String,
    onChoose: () -> Unit,
    modifier: Modifier = Modifier
) {
    val image = avatars.firstOrNull { it.text("id") == avatar }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        AvatarArtwork(name, image?.text("url"), Modifier.size(56.dp).clip(RoundedCornerShape(8.dp)).background(TvColors.Background))
        Action(image?.text("name") ?: "Choose profile image", onChoose, modifier)
    }
}

/** Rendered instead of the form inside its existing Dialog, never a second dialog/window. */
@Composable
internal fun AvatarPicker(
    avatars: List<JSONObject>,
    selected: String,
    onChoose: (String) -> Unit,
    onCancel: () -> Unit
) {
    val choices = remember(avatars) { listOf(JSONObject().put("id", "").put("name", "Use initials")) + avatars }
    val selectedIndex = choices.indexOfFirst { it.text("id") == selected }.coerceAtLeast(0)
    val first = remember { FocusRequester() }
    val grid = rememberLazyGridState(initialFirstVisibleItemIndex = selectedIndex)
    Column(
        Modifier.width(600.dp).heightIn(max = 460.dp)
            .background(TvColors.Panel, RoundedCornerShape(16.dp)).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Heading("Choose profile image")
        LazyVerticalGrid(
            columns = GridCells.Fixed(5), state = grid,
            modifier = Modifier.fillMaxWidth().weight(1f),
            contentPadding = PaddingValues(6.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp)
        ) {
            items(choices, key = { it.text("id") }) { image ->
                val id = image.text("id")
                val active = id == selected
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Card(
                        onClick = { onChoose(id) },
                        modifier = Modifier.fillMaxWidth().aspectRatio(1f)
                            .then(if (image === choices[selectedIndex]) Modifier.focusRequester(first) else Modifier)
                            .semantics { contentDescription = image.text("name") + if (active) ", selected" else "" },
                        colors = CardDefaults.colors(containerColor = TvColors.Background, focusedContainerColor = TvColors.Background),
                        scale = CardDefaults.scale(focusedScale = 1.03f),
                        shape = CardDefaults.shape(shape = RoundedCornerShape(8.dp)),
                        border = CardDefaults.border(
                            border = Border(BorderStroke(if (active) 2.dp else 0.dp, if (active) TvColors.Accent else Color.Transparent)),
                            focusedBorder = Border(BorderStroke(2.dp, Color.White))
                        )
                    ) {
                        Box(Modifier.fillMaxSize()) {
                            AvatarArtwork(if (id.isEmpty()) "A" else image.text("name"), image.text("url"), Modifier.fillMaxSize())
                            if (active) Glyph("check", Modifier.align(Alignment.BottomEnd).background(TvColors.Background).padding(3.dp).size(18.dp), Color.White)
                        }
                    }
                    Text(image.text("name"), style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        Action("Cancel", onCancel)
    }
    LaunchedEffect(Unit) { first.requestFocus() }
}
