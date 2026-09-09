package dev.madari.tv.feature.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.madari.tv.core.objects
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.components.Input
import dev.madari.tv.ui.theme.TvColors

@Composable
fun SettingsScreen(state: TvState, vm: TvViewModel) {
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

/** Browser-based settings handoff, reachable from a phone or laptop on the same network. */
@Composable
fun WebSettingsCard(state: TvState, vm: TvViewModel) {
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
