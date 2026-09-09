package dev.madari.tv.feature.profiles

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.tv.material3.*
import dev.madari.tv.core.text
import dev.madari.tv.state.TvState
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.BrandLogo
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.components.Input
import dev.madari.tv.ui.theme.TvColors
import org.json.JSONObject

@Composable
fun ProfilesScreen(state: TvState, vm: TvViewModel) {
    var selected by remember { mutableStateOf<JSONObject?>(null) }
    var name by remember { mutableStateOf("") }
    var pin by remember { mutableStateOf("") }
    val first=remember { FocusRequester() }
    var focusedProfileCount by remember { mutableIntStateOf(-1) }
    val profiles=remember(state.profiles,state.activeKids) { state.profiles.filter { state.activeKids.isEmpty() || it.text("id")==state.activeKids } }
    Box(Modifier.fillMaxSize().background(Color(0xFF090A0D))) {
        Box(Modifier.fillMaxSize().background(Brush.horizontalGradient(listOf(Color.Transparent,Color(0xFF191018)))))
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
                    Action("Continue",{vm.unlock(profile,pin)},enabled=!state.loading && pin.isNotEmpty(),primary=true)
                    Action("Cancel",{selected=null;pin=""})
                }
            }
            LaunchedEffect(Unit) { pinFocus.requestFocus() }
        }
    }
}
