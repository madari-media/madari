package dev.madari.tv.feature.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.media3.ui.AspectRatioFrameLayout
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.madari.tv.core.objects
import dev.madari.tv.core.strings
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.components.Input
import dev.madari.tv.ui.theme.TvColors
import org.json.JSONArray
import org.json.JSONObject

private val TRACK_PREFERENCES = listOf(
    "subtitle_sdh" to "SDH subtitles · dialogue and sound descriptions",
    "subtitle_forced" to "Forced subtitles · translated foreign dialogue",
    "audio_description" to "Audio description · narrated visual action",
    "audio_commentary" to "Audio commentary"
)

/** Curated list offered when adding a language; unknown tags still survive in the stored list. */
private val COMMON_LANGUAGES = listOf(
    "en","hi","ta","te","ml","kn","mr","bn","gu","pa","ur","as","or","ne","si",
    "fr","de","es","pt","it","nl","pl","ru","uk","tr","sv","da","fi","no","el",
    "ja","ko","zh","ar","he","fa","th","vi","id","ms","tl"
)

/** Mirrors the Linux language label so both clients show the same names. */
private fun languageName(code: String): String {
    val normalized = code.trim().replace('_', '-').lowercase()
    val tags = normalized.split('-')
    val name = when (tags.firstOrNull().orEmpty()) {
        "en", "eng" -> "English"; "hi", "hin" -> "Hindi"; "ta", "tam" -> "Tamil"
        "te", "tel" -> "Telugu"; "ml", "mal" -> "Malayalam"; "kn", "kan" -> "Kannada"
        "mr", "mar" -> "Marathi"; "bn", "ben" -> "Bengali"; "gu", "guj" -> "Gujarati"
        "pa", "pan" -> "Punjabi"; "ur", "urd" -> "Urdu"; "as", "asm" -> "Assamese"
        "or", "ori" -> "Odia"; "ne", "nep" -> "Nepali"; "si", "sin" -> "Sinhala"
        "fr", "fra", "fre" -> "French"; "de", "deu", "ger" -> "German"; "es", "spa" -> "Spanish"
        "pt", "por" -> "Portuguese"; "it", "ita" -> "Italian"; "nl", "nld", "dut" -> "Dutch"
        "pl", "pol" -> "Polish"; "ru", "rus" -> "Russian"; "uk", "ukr" -> "Ukrainian"
        "tr", "tur" -> "Turkish"; "sv", "swe" -> "Swedish"; "da", "dan" -> "Danish"
        "fi", "fin" -> "Finnish"; "no", "nor" -> "Norwegian"; "el", "ell", "gre" -> "Greek"
        "ja", "jpn" -> "Japanese"; "ko", "kor" -> "Korean"; "zh", "zho", "chi" -> "Chinese"
        "ar", "ara" -> "Arabic"; "he", "heb", "iw" -> "Hebrew"; "fa", "fas", "per" -> "Persian"
        "th", "tha" -> "Thai"; "vi", "vie" -> "Vietnamese"; "id", "ind" -> "Indonesian"
        "ms", "msa", "may" -> "Malay"; "tl", "tgl", "fil" -> "Filipino"
        "" -> ""; else -> code.trim()
    }
    if (name.isEmpty()) return ""
    val qualifiers = tags.drop(1).map {
        when (it) { "us" -> "United States"; "gb" -> "United Kingdom"; "br" -> "Brazil"; "pt" -> "Portugal"
            "hans" -> "Simplified"; "hant" -> "Traditional"; else -> it.uppercase() }
    }
    return if (qualifiers.isEmpty()) name else "$name (${qualifiers.joinToString(", ")})"
}

private fun trackLabel(value: String) = when (value) {
    "prefer" -> "Prefer"
    "avoid" -> "Avoid when possible"
    else -> "No preference"
}

private fun nextTrack(value: String) = when (value) {
    "any" -> "prefer"
    "prefer" -> "avoid"
    else -> "any"
}

@Composable
fun SettingsScreen(state: TvState, vm: TvViewModel) {
    var pin by remember { mutableStateOf("") }
    var manifest by rememberSaveable { mutableStateOf("") }
    var local by rememberSaveable { mutableStateOf(false) }
    var removing by remember { mutableStateOf<String?>(null) }
    var configuring by remember { mutableStateOf<String?>(null) }
    var sharing by remember { mutableStateOf<String?>(null) }
    val addons = state.snapshot.optJSONArray("addons").objects()
    LaunchedEffect(state.settingsUnlocked) { if (state.settingsUnlocked) vm.refreshTrakt() }
    LazyColumn(Modifier.fillMaxSize().padding(32.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
        item { Heading("Settings") }
        item { ProfileCard(state, vm, pin) { pin = it } }
        if (!state.settingsUnlocked) {
            item { LockCard(state, vm, pin) { pin = it } }
        } else {
            item { Heading("Playback") }
            item { PlaybackCard(state, vm) }
            item { Heading("Player") }
            item { PlayerCard(vm) }
            item { Heading("Trakt") }
            item { TraktCard(state, vm) }
            item { Heading("Your addons") }
            items(addons, key = { it.text("installation_id") }) { addon ->
                AddonCard(addon, vm, onRemove = { removing = it }, onConfigure = { configuring = addon.text("installation_id") }, onShare = { sharing = addon.text("installation_id") })
            }
            item { AddonInstallCard(manifest, { manifest = it }, local, { local = !local }, vm) }
        }
        item { WebSettingsCard(state, vm) }
        item { Hint("Madari TV 0.1.0 · Kotlin / Compose for TV\nPowered by the same Madari core as the Linux app.") }
    }
    removing?.let { id ->
        val addon = addons.firstOrNull { it.text("installation_id") == id }
        ConfirmRemoveDialog(addon?.getJSONObject("manifest")?.text("name").orEmpty()) { confirmed ->
            if (confirmed && addon != null) vm.removeAddon(addon)
            removing = null
        }
    }
    configuring?.let { id ->
        val addon = addons.firstOrNull { it.text("installation_id") == id }
        if (addon == null) configuring = null else {
            var linked by remember(id) { mutableStateOf<List<String>>(emptyList()) }
            LaunchedEffect(id) { linked = vm.linkedProfiles(addon) }
            ConfigureAddonDialog(addon.getJSONObject("manifest").text("name"), linked, onSave = { url, allowLocal -> vm.configureAddon(id, url, allowLocal); configuring = null }, onDismiss = { configuring = null })
        }
    }
    sharing?.let { id ->
        val addon = addons.firstOrNull { it.text("installation_id") == id }
        if (addon == null) sharing = null else {
            val targets = state.profiles.filter { it.text("id") != state.profile?.text("id") }
            ShareAddonDialog(addon.getJSONObject("manifest").text("name"), targets, onShare = { target, targetPin -> vm.shareAddon(addon, target, targetPin); sharing = null }, onDismiss = { sharing = null })
        }
    }
}

/**
 * The picker owns profile creation and editing. Settings only shows which profile is
 * active and offers a clearly-scoped switch, so profile CRUD is not duplicated here.
 */
@Composable
private fun ProfileCard(state: TvState, vm: TvViewModel, pin: String, onPin: (String) -> Unit) {
    val kids = state.profile?.optBoolean("kids") == true
    Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(state.profile?.text("name").orEmpty(), style = MaterialTheme.typography.titleLarge)
        Hint(if (kids) "Kids profile · protected by the guardian's PIN. Create and edit profiles from the profile picker." else "Create and edit profiles from the profile picker.")
        if (kids) Input("Guardian PIN", pin, onPin, Modifier.width(360.dp), secret = true)
        Action("Switch profile", { vm.leave(pin) }, enabled = !state.loading)
    }
}

@Composable
private fun LockCard(state: TvState, vm: TvViewModel, pin: String, onPin: (String) -> Unit) {
    val kids = state.profile?.optBoolean("kids") == true
    Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Settings are locked", style = MaterialTheme.typography.titleLarge)
        Hint(if (kids) "Enter the guardian's PIN to change this kids profile." else "Enter this profile's PIN, or leave it empty when no PIN is set.")
        Input("PIN", pin, onPin, Modifier.width(360.dp), secret = true)
        Action("Unlock settings", { vm.authorize(pin) }, enabled = !state.loading, primary = true)
    }
}

/** Per-profile playback defaults. Mirrors Linux Settings → Playback. */
@Composable
private fun PlaybackCard(state: TvState, vm: TvViewModel) {
    val prefs = state.snapshot.optJSONObject("playback_preferences") ?: JSONObject()
    fun save(mutate: (JSONObject) -> Unit) {
        val next = JSONObject(prefs.toString())
        mutate(next)
        vm.setPreferences(next)
    }
    var editing by remember { mutableStateOf<String?>(null) }
    Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Hint("Saved per profile and applied when a video opens. Tracks can still be changed while watching.")
        Action((if (prefs.optBoolean("subtitles_enabled", true)) "✓ " else "") + "Subtitles by default", {
            save { it.put("subtitles_enabled", !prefs.optBoolean("subtitles_enabled", true)) }
        })
        for ((key, label) in TRACK_PREFERENCES) {
            Action("$label · ${trackLabel(prefs.optString(key, "any"))}", {
                save { it.put(key, nextTrack(prefs.optString(key, "any"))) }
            })
        }
        LanguageRow("Audio languages", prefs.strings("audio_languages")) { editing = "audio_languages" }
        LanguageRow("Subtitle languages", prefs.strings("subtitle_languages")) { editing = "subtitle_languages" }
    }
    editing?.let { key ->
        LanguageDialog(
            title = if (key == "audio_languages") "Audio languages" else "Subtitle languages",
            selected = prefs.strings(key),
            onSave = { list -> save { it.put(key, JSONArray(list)) }; editing = null },
            onDismiss = { editing = null }
        )
    }
}

@Composable
private fun LanguageRow(label: String, languages: List<String>, onEdit: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(label, style = MaterialTheme.typography.titleMedium)
            Hint(if (languages.isEmpty()) "No preference · the video's default is used" else languages.joinToString("  ·  ") { languageName(it) })
        }
        Action("Edit", onEdit)
    }
}

/** TV-friendly ordered language editor; the first available language wins. */
@Composable
private fun LanguageDialog(title: String, selected: List<String>, onSave: (List<String>) -> Unit, onDismiss: () -> Unit) {
    var list by remember { mutableStateOf(selected) }
    val first = remember { FocusRequester() }
    Dialog(onDismissRequest = onDismiss) {
        Column(Modifier.width(720.dp).background(TvColors.Panel).padding(28.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Heading(title)
            Hint("First available language wins. Move languages up or down to set their priority.")
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                val save = remember { FocusRequester() }
                Action("Save", { onSave(list) }, Modifier.focusRequester(save), primary = true)
                Action("Cancel", onDismiss)
                LaunchedEffect(Unit) { save.requestFocus() }
            }
            LazyColumn(Modifier.fillMaxWidth().heightIn(max = 460.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (list.isEmpty()) item { Hint("No preference. Add a language below.") }
                itemsIndexed(list) { index, code ->
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text(languageName(code), Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                        Action("Up", { list = list.toMutableList().apply { add(index - 1, removeAt(index)) } }, enabled = index > 0)
                        Action("Down", { list = list.toMutableList().apply { add(index + 1, removeAt(index)) } }, enabled = index < list.lastIndex)
                        Action("Remove", { list = list.toMutableList().apply { removeAt(index) } })
                    }
                }
                item { Hint("Add a language") }
                items(COMMON_LANGUAGES.filter { it !in list }) { code ->
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text(languageName(code), Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                        Action("Add", { list = list + code })
                    }
                }
            }
        }
    }
}

/** Device-wide player display defaults (Linux keeps these beside the player too). */
@Composable
private fun PlayerCard(vm: TvViewModel) {
    var subtitleSize by remember { mutableStateOf(vm.playerSubtitleSize()) }
    var resize by remember { mutableIntStateOf(vm.playerResizeMode()) }
    var speed by remember { mutableFloatStateOf(vm.playerSpeed()) }
    Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Hint("Device-wide defaults. They can still be changed while watching.")
        Text("Subtitle size", style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            for ((label, value) in listOf("Small" to "small", "Medium" to "medium", "Large" to "large")) {
                Action((if (subtitleSize == value) "✓ " else "") + label, { subtitleSize = value; vm.setPlayerPreference("player_subtitle_size", value) })
            }
        }
        Text("Picture size", style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            for ((label, value) in listOf("Fit" to AspectRatioFrameLayout.RESIZE_MODE_FIT, "Zoom" to AspectRatioFrameLayout.RESIZE_MODE_ZOOM, "Stretch" to AspectRatioFrameLayout.RESIZE_MODE_FILL)) {
                Action((if (resize == value) "✓ " else "") + label, { resize = value; vm.setPlayerPreference("player_resize", value) })
            }
        }
        Text("Default speed", style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            for (value in listOf(.5f, .75f, 1f, 1.25f, 1.5f, 2f)) {
                Action("${if (speed == value) "✓ " else ""}${value}×", { speed = value; vm.setPlayerPreference("player_speed", value) })
            }
        }
    }
}

/** Per-profile Trakt connection. Credentials are entered once and stored on the TV. */
@Composable
private fun TraktCard(state: TvState, vm: TvViewModel) {
    var setup by remember { mutableStateOf(false) }
    val trakt = state.trakt
    val device = state.traktDevice
    Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        when {
            device.length() > 0 -> {
                Hint("Enter this code on Trakt to connect. It expires in ${(device.optLong("expires_in") / 60).coerceAtLeast(1)} minutes.")
                Text(device.text("user_code"), color = TvColors.Accent, style = MaterialTheme.typography.headlineMedium)
                Text(device.text("verification_url"), style = MaterialTheme.typography.bodyLarge)
                Action("Cancel", vm::traktCancel)
            }
            trakt.optBoolean("connected") -> {
                Hint("Connected as ${trakt.text("username").ifEmpty { "your account" }} · ${trakt.optInt("lists")} lists")
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Action("Sync now", vm::traktSync)
                    Action("Disconnect", { vm.traktDisconnect() })
                }
            }
            else -> {
                Hint("Import your watchlist, watched history and personal lists. Playback reports progress to Trakt.")
                Action("Connect Trakt", { setup = true }, primary = true)
            }
        }
    }
    if (setup) {
        val saved = vm.traktCredentials()
        var clientId by remember { mutableStateOf(saved.first) }
        var clientSecret by remember { mutableStateOf(saved.second) }
        var redirect by remember { mutableStateOf(saved.third) }
        Dialog(onDismissRequest = { setup = false }) {
            Column(Modifier.width(680.dp).background(TvColors.Panel).padding(28.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Heading("Connect Trakt")
                Hint("Create a Trakt application at trakt.tv/oauth/applications, then paste its credentials. They are stored only on this TV.")
                Input("Client ID", clientId, { clientId = it })
                Input("Client secret", clientSecret, { clientSecret = it }, secret = true)
                Input("Redirect URI", redirect, { redirect = it })
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Action("Continue", { vm.traktConnect(clientId, clientSecret, redirect); setup = false }, enabled = clientId.isNotBlank() && clientSecret.isNotBlank(), primary = true)
                    Action("Cancel", { setup = false })
                }
            }
        }
    }
}

@Composable
private fun AddonCard(addon: JSONObject, vm: TvViewModel, onRemove: (String) -> Unit, onConfigure: () -> Unit, onShare: () -> Unit) {
    Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Text(addon.getJSONObject("manifest").text("name"), style = MaterialTheme.typography.titleLarge)
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Action(if (addon.optBoolean("enabled")) "Disable" else "Enable", { vm.enable(addon) })
            Action("Move up", { vm.moveAddon(addon, -1) })
            Action("Move down", { vm.moveAddon(addon, 1) })
        }
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Action("Configure", onConfigure)
            Action("Share…", onShare)
            Action("Remove", { onRemove(addon.text("installation_id")) })
        }
    }
}

/** Reconfiguring a shared installation affects every linked profile. */
@Composable
private fun ConfigureAddonDialog(name: String, linked: List<String>, onSave: (String, Boolean) -> Unit, onDismiss: () -> Unit) {
    var url by remember { mutableStateOf("") }
    var local by remember { mutableStateOf(false) }
    val focus = remember { FocusRequester() }
    Dialog(onDismissRequest = onDismiss) {
        Column(Modifier.width(640.dp).background(TvColors.Panel).padding(28.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Heading("Configure $name")
            Hint(if (linked.isEmpty()) "Paste the complete configured manifest URL." else "Changes apply to every linked profile: ${linked.joinToString(", ")}.")
            Input("Manifest URL", url, { url = it }, Modifier.focusRequester(focus))
            Action(if (local) "✓ Allow local-network access" else "Allow local-network access", { local = !local })
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Action("Save", { onSave(url, local) }, enabled = url.isNotBlank(), primary = true)
                Action("Cancel", onDismiss)
            }
            LaunchedEffect(Unit) { focus.requestFocus() }
        }
    }
}

/** Links one installation to another profile; its PIN or guardian PIN is required. */
@Composable
private fun ShareAddonDialog(name: String, targets: List<JSONObject>, onShare: (String, String) -> Unit, onDismiss: () -> Unit) {
    var target by remember(targets) { mutableStateOf(targets.firstOrNull()?.text("id").orEmpty()) }
    var pin by remember { mutableStateOf("") }
    Dialog(onDismissRequest = onDismiss) {
        Column(Modifier.width(640.dp).background(TvColors.Panel).padding(28.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Heading("Share $name")
            if (targets.isEmpty()) {
                Hint("Create another profile before sharing an addon.")
                Action("Close", onDismiss)
            } else {
                Hint("This links one installation. Future configuration changes affect every linked profile. Library and progress stay separate.")
                for (profile in targets) {
                    Action((if (profile.text("id") == target) "✓ " else "") + profile.text("name"), { target = profile.text("id") })
                }
                Input("Recipient or guardian PIN, if required", pin, { pin = it }, Modifier.width(420.dp), secret = true)
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Action("Share", { onShare(target, pin) }, enabled = target.isNotBlank(), primary = true)
                    Action("Cancel", onDismiss)
                }
            }
        }
    }
}

@Composable
private fun AddonInstallCard(manifest: String, onManifest: (String) -> Unit, local: Boolean, onToggleLocal: () -> Unit, vm: TvViewModel) {
    Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Text("Install addon", style = MaterialTheme.typography.titleLarge)
        Input("Addon manifest URL", manifest, onManifest)
        Action(if (local) "✓ Allow local-network addon" else "Allow local-network addon", onToggleLocal)
        Action("Install addon", { vm.install(manifest, local) }, enabled = manifest.isNotBlank(), primary = true)
    }
}

@Composable
private fun ConfirmRemoveDialog(name: String, onResult: (Boolean) -> Unit) {
    val confirm = remember { FocusRequester() }
    Dialog(onDismissRequest = { onResult(false) }) {
        Column(Modifier.width(520.dp).background(TvColors.Panel).padding(28.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Heading("Remove addon?")
            Text(if (name.isBlank()) "Remove this addon from your profile?" else "Remove $name from your profile?", style = MaterialTheme.typography.bodyLarge)
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Action("Remove", { onResult(true) }, Modifier.focusRequester(confirm), primary = true)
                Action("Cancel", { onResult(false) })
            }
            LaunchedEffect(Unit) { confirm.requestFocus() }
        }
    }
}

/** Browser-based settings handoff, reachable from a phone or laptop on the same network. */
@Composable
fun WebSettingsCard(state: TvState, vm: TvViewModel) {
    Column(Modifier.fillMaxWidth().background(TvColors.Panel).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Manage from your phone or laptop", style = MaterialTheme.typography.titleLarge)
        if (state.web.optBoolean("running")) {
            Text(state.webAddress, color = TvColors.Accent, style = MaterialTheme.typography.headlineSmall)
            Text("Pairing code: ${state.web.text("code")}", style = MaterialTheme.typography.titleLarge)
            Hint("Open this address on the same network. Keep Madari open on the TV.")
        } else Hint("Start web settings to manage profiles, addon URLs and playback preferences in a browser.")
        Action(if (state.web.optBoolean("running")) "Stop web settings" else "Start web settings", vm::toggleWeb)
    }
}
