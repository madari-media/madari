package dev.madari.tv.feature.profiles

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.tv.material3.Border
import androidx.tv.material3.Card
import androidx.tv.material3.CardDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import coil.compose.AsyncImage
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.BrandLogo
import dev.madari.tv.ui.components.Glyph
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.components.Input
import dev.madari.tv.ui.theme.TvColors
import org.json.JSONObject

/** Poster-grid wallpapers published by madari-media/poster-generator. */
private const val ProfileWallpaper = "https://downloads.madari.media/backgrounds/webp/desktop_fhd.webp"
/** A remote-first picker with a quiet reading area over the poster wallpaper. */
@Composable
fun ProfilesScreen(state: TvState, vm: TvViewModel) {
    val profiles = remember(state.profiles, state.activeKids) {
        state.profiles.filter { state.activeKids.isEmpty() || it.text("id") == state.activeKids }
    }
    var selected by remember { mutableStateOf<JSONObject?>(null) }
    var pin by remember { mutableStateOf("") }
    var adding by remember { mutableStateOf(false) }
    var name by remember { mutableStateOf("") }
    var newPin by remember { mutableStateOf("") }
    var guardianPin by remember { mutableStateOf("") }
    var kids by remember { mutableStateOf(false) }
    var avatar by remember { mutableStateOf("") }
    var editAvatar by remember { mutableStateOf("") }
    var editing by remember { mutableStateOf<JSONObject?>(null) }
    var editName by remember { mutableStateOf("") }
    var editPin by remember { mutableStateOf("") }
    var editNewPin by remember { mutableStateOf("") }
    val first = remember { FocusRequester() }
    var focusedCount by remember { mutableIntStateOf(-1) }

    Box(Modifier.fillMaxSize().background(TvColors.Background)) {
        AsyncImage(ProfileWallpaper, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        Box(
            Modifier.fillMaxSize().background(
                Brush.horizontalGradient(
                    0f to TvColors.Background,
                    .38f to TvColors.Background.copy(alpha = .96f),
                    .68f to TvColors.Background.copy(alpha = .65f),
                    1f to TvColors.Background.copy(alpha = .3f)
                )
            )
        )
        Box(
            Modifier.fillMaxSize().background(
                Brush.verticalGradient(
                    0f to TvColors.Background.copy(alpha = .35f),
                    .5f to Color.Transparent,
                    1f to TvColors.Background.copy(alpha = .85f)
                )
            )
        )

        Column(Modifier.fillMaxSize().padding(horizontal = 64.dp, vertical = 36.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                BrandLogo(Modifier.size(32.dp))
                Text("madari", style = MaterialTheme.typography.titleLarge, color = Color.White)
            }
            Column(
                Modifier.fillMaxWidth().weight(1f).padding(vertical = 28.dp),
                verticalArrangement = Arrangement.Center
            ) {
                Text(
                    if (profiles.isEmpty() && !state.loading) "Make yourself at home" else "Who's watching?",
                    style = MaterialTheme.typography.displaySmall.copy(
                        fontSize = 34.sp, lineHeight = 40.sp, fontWeight = FontWeight.Medium
                    ),
                    color = Color.White
                )
                Spacer(Modifier.height(10.dp))
                Hint(if (profiles.isEmpty() && !state.loading) "Create a profile to get started." else "Pick a profile to continue.")
                Spacer(Modifier.height(28.dp))

                if (state.loading && profiles.isEmpty()) {
                    Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                        repeat(3) { Box(Modifier.size(112.dp).background(TvColors.Panel, RoundedCornerShape(12.dp))) }
                    }
                    Spacer(Modifier.height(12.dp))
                    Hint("Loading profiles…")
                } else {
                    // Five profiles plus the add tile fit in the 832dp TV content area.
                    LazyRow(
                        Modifier.fillMaxWidth(),
                        contentPadding = PaddingValues(6.dp),
                        horizontalArrangement = Arrangement.spacedBy(24.dp)
                    ) {
                        items(profiles, key = { it.text("id") }) { profile ->
                            ProfileCard(
                                profile = profile,
                                modifier = if (profile === profiles.firstOrNull()) Modifier.focusRequester(first) else Modifier,
                                onOpen = {
                                    if (!state.loading) {
                                        if (profile.optBoolean("pin_protected")) { selected = profile; pin = "" }
                                        else vm.unlock(profile, "")
                                    }
                                },
                                onEdit = { editing = profile; editName = profile.text("name"); editPin = ""; editNewPin = ""; editAvatar = profile.text("avatar") },
                                avatarUrl = state.profileAvatars.firstOrNull { it.text("id") == profile.text("avatar") }?.text("url")
                            )
                        }
                        item("add-profile") {
                            ProfileCard(
                                profile = null,
                                modifier = if (profiles.isEmpty()) Modifier.focusRequester(first) else Modifier,
                                onOpen = {
                                    if (!state.loading) {
                                        adding = true; name = ""; newPin = ""; guardianPin = ""; kids = false; avatar = ""
                                    }
                                }
                            )
                        }
                    }
                }
            }
            Text(
                if (profiles.isEmpty()) "A little space for everything you love."
                else "Press OK to select  ·  Hold OK to edit a profile",
                style = MaterialTheme.typography.bodyMedium.copy(fontSize = 16.sp),
                color = TvColors.Muted
            )
        }
    }

    LaunchedEffect(state.loading, profiles.size) {
        if (!state.loading && focusedCount != profiles.size) { first.requestFocus(); focusedCount = profiles.size }
    }

    selected?.let { profile ->
        Dialog(onDismissRequest = { selected = null; pin = "" }) {
            val pinFocus = remember { FocusRequester() }
            Column(
                Modifier.width(430.dp).heightIn(max = 460.dp)
                    .background(TvColors.Panel, RoundedCornerShape(16.dp))
                    .border(1.dp, Color.White.copy(alpha = .12f), RoundedCornerShape(16.dp))
                    .verticalScroll(rememberScrollState()).padding(28.dp),
                verticalArrangement = Arrangement.spacedBy(20.dp)
            ) {
                Heading(profile.text("name"))
                Hint("Enter your PIN to continue.")
                Input("PIN", pin, { pin = it }, Modifier.focusRequester(pinFocus), secret = true)
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Action("Continue", { val target = profile; selected = null; vm.unlock(target, pin); pin = "" }, enabled = !state.loading && pin.isNotEmpty(), primary = true)
                    Action("Cancel", { selected = null; pin = "" })
                }
            }
            LaunchedEffect(Unit) { pinFocus.requestFocus() }
        }
    }

    if (adding) {
        val guardian = profiles.firstOrNull { !it.optBoolean("kids") }
        var choosingImage by remember { mutableStateOf(false) }
        Dialog(onDismissRequest = { if (choosingImage) choosingImage = false else adding = false }) {
            val focus = remember { FocusRequester() }
            val imageFocus = remember { FocusRequester() }
            var returnToImage by remember { mutableStateOf(false) }
            if (choosingImage) AvatarPicker(state.profileAvatars, avatar, { avatar = it; choosingImage = false }, { choosingImage = false })
            else Column(
                Modifier.width(480.dp).heightIn(max = 460.dp)
                    .background(TvColors.Panel, RoundedCornerShape(16.dp))
                    .border(1.dp, Color.White.copy(alpha = .12f), RoundedCornerShape(16.dp))
                    .verticalScroll(rememberScrollState()).padding(28.dp),
                verticalArrangement = Arrangement.spacedBy(18.dp)
            ) {
                Heading("New profile")
                AvatarField(avatar, state.profileAvatars, name, { returnToImage = true; choosingImage = true }, Modifier.focusRequester(imageFocus))
                if (guardian != null && guardian.optBoolean("pin_protected")) {
                    Input("PIN for ${guardian.text("name")}", guardianPin, { guardianPin = it }, secret = true)
                }
                Input("Profile name", name, { name = it }, Modifier.focusRequester(focus))
                Input("New profile PIN (optional)", newPin, { newPin = it }, secret = true)
                if (guardian != null) Action(if (kids) "✓ Kids profile" else "Kids profile", { kids = !kids })
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Action(
                        "Create profile",
                        {
                            if (guardian == null) vm.createProfile(name, newPin, false, avatar)
                            else vm.addProfile(guardian, guardianPin, name, newPin, kids, avatar)
                            adding = false
                        },
                        enabled = name.isNotBlank() && !state.loading, primary = true
                    )
                    Action("Cancel", { adding = false })
                }
            }
            LaunchedEffect(choosingImage) {
                if (!choosingImage) (if (returnToImage) imageFocus else focus).requestFocus()
            }
        }
    }

    editing?.let { profile ->
        var choosingImage by remember { mutableStateOf(false) }
        Dialog(onDismissRequest = { if (choosingImage) choosingImage = false else editing = null }) {
            val focus = remember { FocusRequester() }
            val imageFocus = remember { FocusRequester() }
            var returnToImage by remember { mutableStateOf(false) }
            if (choosingImage) AvatarPicker(state.profileAvatars, editAvatar, { editAvatar = it; choosingImage = false }, { choosingImage = false })
            else Column(
                Modifier.width(480.dp).heightIn(max = 460.dp)
                    .background(TvColors.Panel, RoundedCornerShape(16.dp))
                    .border(1.dp, Color.White.copy(alpha = .12f), RoundedCornerShape(16.dp))
                    .verticalScroll(rememberScrollState()).padding(28.dp),
                verticalArrangement = Arrangement.spacedBy(18.dp)
            ) {
                Heading("Edit profile")
                AvatarField(editAvatar, state.profileAvatars, editName, { returnToImage = true; choosingImage = true }, Modifier.focusRequester(imageFocus))
                if (profile.optBoolean("pin_protected")) {
                    Input("Current PIN", editPin, { editPin = it }, Modifier.focusRequester(focus), secret = true)
                }
                Input("Name", editName, { editName = it }, if (profile.optBoolean("pin_protected")) Modifier else Modifier.focusRequester(focus))
                Input("New PIN (leave empty to keep)", editNewPin, { editNewPin = it }, secret = true)
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Action(
                        "Save",
                        { vm.updateProfile(profile, editPin, editName, editNewPin, editAvatar); editing = null },
                        enabled = editName.isNotBlank() && !state.loading, primary = true
                    )
                    Action("Cancel", { editing = null })
                }
            }
            LaunchedEffect(choosingImage) {
                if (!choosingImage) (if (returnToImage) imageFocus else focus).requestFocus()
            }
        }
    }
}

@Composable
internal fun ProfileCard(
    profile: JSONObject?,
    modifier: Modifier = Modifier,
    onOpen: () -> Unit,
    onEdit: (() -> Unit)? = null,
    avatarUrl: String? = null
) {
    var focused by remember { mutableStateOf(false) }
    val name = profile?.text("name") ?: "Add profile"
    val tint = remember(profile?.text("id")) {
        val palette = listOf(Color(0xFF426A88), Color(0xFF87634F), Color(0xFF605D88), Color(0xFF44796E), Color(0xFF895770))
        if (profile == null) TvColors.Panel else palette[Math.floorMod(profile.text("id").hashCode(), palette.size)]
    }
    Column(
        modifier.width(112.dp).onFocusChanged { focused = it.hasFocus },
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        Card(
            onClick = onOpen,
            onLongClick = onEdit,
            modifier = Modifier.size(112.dp).semantics {
                contentDescription = listOfNotNull(
                    name,
                    if (profile?.optBoolean("kids") == true) "Kids" else null,
                    if (profile?.optBoolean("pin_protected") == true) "PIN protected" else null
                ).joinToString(", ")
            },
            scale = CardDefaults.scale(focusedScale = 1.025f),
            shape = CardDefaults.shape(shape = RoundedCornerShape(12.dp)),
            colors = CardDefaults.colors(containerColor = tint, focusedContainerColor = tint),
            border = CardDefaults.border(
                border = Border(BorderStroke(1.dp, Color.White.copy(alpha = .12f)), shape = RoundedCornerShape(12.dp)),
                focusedBorder = Border(BorderStroke(2.dp, Color.White), shape = RoundedCornerShape(12.dp))
            )
        ) {
            Box(
                Modifier.fillMaxSize().background(Brush.linearGradient(listOf(Color.White.copy(alpha = .08f), Color.Black.copy(alpha = .2f)))),
                contentAlignment = Alignment.Center
            ) {
                if (profile == null) {
                    Glyph("plus", Modifier.size(36.dp), if (focused) Color.White else TvColors.Muted)
                } else {
                    AvatarArtwork(name, avatarUrl, Modifier.fillMaxSize().clearAndSetSemantics { })
                    if (profile.optBoolean("pin_protected")) {
                        Box(
                            Modifier.align(Alignment.BottomEnd).padding(8.dp)
                                .background(TvColors.Background.copy(alpha = .65f), RoundedCornerShape(6.dp)).padding(5.dp)
                        ) { Glyph("lock", Modifier.size(16.dp), Color.White) }
                    }
                }
            }
        }
        Text(
            name,
            modifier = Modifier.fillMaxWidth().clearAndSetSemantics { },
            style = MaterialTheme.typography.bodyLarge.copy(fontSize = 18.sp),
            color = if (focused) Color.White else TvColors.Muted,
            textAlign = TextAlign.Center,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis
        )
    }
}
