package dev.madari.tv.feature.profiles

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.tv.material3.*
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

private sealed interface AddFlow {
    data object Guardian : AddFlow
    data class Create(val guardian: JSONObject) : AddFlow
}

@Composable
fun ProfilesScreen(state: TvState, vm: TvViewModel) {
    var selected by remember { mutableStateOf<JSONObject?>(null) }
    var name by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    var addFlow by remember { mutableStateOf<AddFlow?>(null) }
    var guardianPin by remember { mutableStateOf("") }
    var newPin by remember { mutableStateOf("") }
    var kids by remember { mutableStateOf(false) }
    val first=remember { FocusRequester() }
    var focusedProfileCount by remember { mutableIntStateOf(-1) }
    val profiles=remember(state.profiles,state.activeKids) { state.profiles.filter { state.activeKids.isEmpty() || it.text("id")==state.activeKids } }
    Box(Modifier.fillMaxSize().background(Color(0xFF090A0D))) {
        AsyncImage(ProfileWallpaper,null,Modifier.fillMaxSize(),contentScale=ContentScale.Crop)
        Box(Modifier.fillMaxSize().background(Brush.horizontalGradient(listOf(Color(0xFF090A0D).copy(.94f),Color(0xFF090A0D).copy(.72f),Color(0xFF090A0D).copy(.42f)))))
        Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(Color(0xFF090A0D).copy(.7f),Color.Transparent,Color(0xFF090A0D).copy(.88f)))))
        Column(Modifier.fillMaxSize().padding(start=64.dp,top=36.dp,end=64.dp,bottom=32.dp),verticalArrangement=Arrangement.spacedBy(28.dp)) {
            BrandLogo(Modifier.size(38.dp))
            Column(verticalArrangement=Arrangement.spacedBy(10.dp)) {
                Text(if(profiles.isEmpty() && !state.loading) "Make yourself at home" else "Who's watching?",style=MaterialTheme.typography.displayMedium.copy(fontSize=36.sp,lineHeight=42.sp,fontWeight=FontWeight.Medium),color=Color.White)
                Text(if(profiles.isEmpty() && !state.loading) "Create a profile to get started." else "Pick a profile to continue.",style=MaterialTheme.typography.bodyLarge,color=TvColors.Muted)
            }
            if(state.loading && profiles.isEmpty()) {
                repeat(2) { Box(Modifier.width(390.dp).height(82.dp).background(TvColors.Panel.copy(.55f),RoundedCornerShape(10.dp))) }
            } else if(profiles.isEmpty()) {
                Column(Modifier.width(390.dp),verticalArrangement=Arrangement.spacedBy(14.dp)) {
                    Input("Profile name",name,{name=it},Modifier.focusRequester(first))
                    Input("PIN (optional)",pin,{pin=it},secret=true)
                    Action("Create profile",{vm.createProfile(name,pin)},enabled=name.isNotBlank() && !state.loading,primary=true)
                }
            } else {
                LazyRow(Modifier.fillMaxWidth(),contentPadding=PaddingValues(6.dp),horizontalArrangement=Arrangement.spacedBy(24.dp)) {
                    itemsIndexed(profiles,key={_,profile -> profile.text("id")}) { index,profile ->
                        var focused by remember { mutableStateOf(false) }
                        val tint=remember(profile.text("id")) {
                            val palette=listOf(Color(0xFF426A88),Color(0xFF87634F),Color(0xFF605D88),Color(0xFF44796E),Color(0xFF895770))
                            palette[Math.floorMod(profile.text("id").hashCode(),palette.size)]
                        }
                        Column(Modifier.width(104.dp),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(12.dp)) {
                            Card(onClick={if(profile.optBoolean("pin_protected")) { selected=profile;pin="" } else vm.unlock(profile,"")},
                                modifier=Modifier.size(104.dp).then(if(index==0) Modifier.focusRequester(first) else Modifier).onFocusChanged { focused=it.isFocused },
                                scale=CardDefaults.scale(focusedScale=1f),shape=CardDefaults.shape(shape=RoundedCornerShape(6.dp)),
                                colors=CardDefaults.colors(containerColor=tint,focusedContainerColor=tint,focusedContentColor=Color.White),
                                border=CardDefaults.border(focusedBorder=Border(BorderStroke(1.dp,Color.White.copy(.75f)),shape=RoundedCornerShape(6.dp)))) {
                                Box(Modifier.fillMaxSize().background(Brush.linearGradient(listOf(tint,tint.copy(alpha=.45f)))),contentAlignment=Alignment.Center) {
                                    Text(profile.text("name").take(1).uppercase(),style=MaterialTheme.typography.displayMedium,color=Color.White)
                                }
                            }
                            Text(profile.text("name"),style=MaterialTheme.typography.bodyLarge,color=if(focused) Color.White else TvColors.Muted,maxLines=1,overflow=TextOverflow.Ellipsis)
                        }
                    }
                    item("add") {
                        var focused by remember { mutableStateOf(false) }
                        Column(Modifier.width(104.dp),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(12.dp)) {
                            Card(onClick={addFlow=AddFlow.Guardian},modifier=Modifier.size(104.dp).onFocusChanged { focused=it.isFocused },
                                scale=CardDefaults.scale(focusedScale=1f),shape=CardDefaults.shape(shape=RoundedCornerShape(6.dp)),
                                colors=CardDefaults.colors(containerColor=TvColors.Panel,focusedContainerColor=TvColors.Panel,focusedContentColor=Color.White),
                                border=CardDefaults.border(focusedBorder=Border(BorderStroke(1.dp,Color.White.copy(.75f)),shape=RoundedCornerShape(6.dp)))) {
                                Box(Modifier.fillMaxSize(),contentAlignment=Alignment.Center) { Glyph("plus",Modifier.size(34.dp),if(focused) Color.White else TvColors.Muted) }
                            }
                            Text("Add profile",style=MaterialTheme.typography.bodyLarge,color=if(focused) Color.White else TvColors.Muted,maxLines=1,overflow=TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }
    }
    LaunchedEffect(state.loading,profiles.size) {
        if(!state.loading && focusedProfileCount!=profiles.size) { first.requestFocus();focusedProfileCount=profiles.size }
    }
    selected?.let { profile ->
        Dialog(onDismissRequest={selected=null;pin=""}) {
            val pinFocus=remember { FocusRequester() }
            Column(Modifier.width(430.dp).background(TvColors.Panel,RoundedCornerShape(12.dp)).padding(28.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
                Heading(profile.text("name"))
                Hint("Enter your PIN to continue.")
                Input("PIN",pin,{pin=it},Modifier.focusRequester(pinFocus),secret=true)
                Row(horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                    Action("Continue",{val target=profile;selected=null;vm.unlock(target,pin);pin=""},enabled=!state.loading && pin.isNotEmpty(),primary=true)
                    Action("Cancel",{selected=null;pin=""})
                }
            }
            LaunchedEffect(Unit) { pinFocus.requestFocus() }
        }
    }
    addFlow?.let { flow ->
        Dialog(onDismissRequest={addFlow=null;guardianPin="";name="";newPin="";kids=false}) {
            val focus=remember { FocusRequester() }
            Column(Modifier.width(540.dp).background(TvColors.Panel,RoundedCornerShape(12.dp)).padding(28.dp),verticalArrangement=Arrangement.spacedBy(18.dp)) {
                when(flow) {
                    AddFlow.Guardian -> {
                        Heading("Add a profile")
                        Hint("Choose the regular profile that manages profiles.")
                        val adults=state.profiles.filter { !it.optBoolean("kids") }
                        LazyColumn(Modifier.heightIn(max=280.dp),verticalArrangement=Arrangement.spacedBy(10.dp)) {
                            items(adults,key={it.text("id")}) { adult ->
                                Action(adult.text("name"),{addFlow=AddFlow.Create(adult)},Modifier.fillMaxWidth().then(if(adult===adults.firstOrNull()) Modifier.focusRequester(focus) else Modifier))
                            }
                        }
                        Action("Cancel",{addFlow=null})
                    }
                    is AddFlow.Create -> {
                        Heading("New profile")
                        Input("PIN for ${flow.guardian.text("name")}" + if(flow.guardian.optBoolean("pin_protected")) "" else " (optional)",guardianPin,{guardianPin=it},secret=true)
                        Input("Profile name",name,{name=it},Modifier.focusRequester(focus))
                        Input("New profile PIN (optional)",newPin,{newPin=it},secret=true)
                        Action(if(kids) "✓ Kids profile" else "Kids profile",{kids=!kids})
                        Row(horizontalArrangement=Arrangement.spacedBy(12.dp)) {
                            Action("Create profile",{vm.addProfile(flow.guardian,guardianPin,name,newPin,kids);addFlow=null;guardianPin="";name="";newPin="";kids=false},enabled=name.isNotBlank() && !state.loading,primary=true)
                            Action("Back",{addFlow=AddFlow.Guardian})
                            Action("Cancel",{addFlow=null})
                        }
                    }
                }
            }
            LaunchedEffect(flow) { focus.requestFocus() }
        }
    }
}
