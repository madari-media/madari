@file:androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
package dev.madari.tv.feature.player

import android.net.Uri
import android.view.KeyEvent
import android.view.ViewGroup
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.currentStateAsState
import androidx.media3.common.*
import androidx.media3.datasource.*
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.MediaSession
import androidx.media3.ui.AspectRatioFrameLayout
import androidx.media3.ui.PlayerView
import androidx.media3.ui.SubtitleView
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.madari.tv.MainActivity
import dev.madari.tv.core.NativeCore
import dev.madari.tv.core.Playback
import dev.madari.tv.core.Source
import dev.madari.tv.core.obj
import dev.madari.tv.core.objects
import dev.madari.tv.core.text
import dev.madari.tv.state.TvViewModel
import dev.madari.tv.ui.components.Action
import dev.madari.tv.ui.components.Heading
import dev.madari.tv.ui.components.Hint
import dev.madari.tv.ui.theme.TvColors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException

/** Reads directly from the Rust embedded torrent engine, including random access. */
class NativeTorrentDataSource : BaseDataSource(true) {
    private var handle = 0L
    private var currentUri: Uri? = null
    private var remaining = 0L
    override fun getUri() = currentUri
    override fun open(dataSpec: DataSpec): Long {
        transferInitializing(dataSpec)
        try {
            handle = NativeCore.openMedia(dataSpec.uri.toString(),dataSpec.position)
            val available = NativeCore.mediaLength(handle) - dataSpec.position
            remaining = if(dataSpec.length == C.LENGTH_UNSET.toLong()) available else minOf(available,dataSpec.length)
            currentUri = dataSpec.uri
            transferStarted(dataSpec)
            return remaining
        } catch(e: Exception) { if(handle!=0L) NativeCore.closeMedia(handle); handle=0; throw IOException("Could not open torrent stream",e) }
    }
    override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
        if(length==0) return 0
        if(remaining==0L) return C.RESULT_END_OF_INPUT
        val started=android.os.SystemClock.elapsedRealtime()
        while(true) {
            if(Thread.currentThread().isInterrupted) throw java.io.InterruptedIOException("Torrent read cancelled")
            val count=NativeCore.readMedia(handle,buffer,offset,minOf(length.toLong(),remaining).toInt())
            if(count == -2) {
                if(android.os.SystemClock.elapsedRealtime()-started >= 120_000L)
                    throw java.net.SocketTimeoutException("No torrent data arrived for 2 minutes. Peers may be unavailable; try another source.")
                continue
            }
            if(count > 0) { remaining-=count; bytesTransferred(count) }
            return count
        }
    }
    override fun close() {
        if(handle!=0L) { try { NativeCore.closeMedia(handle) } finally { handle=0; currentUri=null; transferEnded() } }
    }
}
class PlaybackDataSourceFactory(private val fallback: DataSource.Factory) : DataSource.Factory {
    override fun createDataSource(): DataSource = object : DataSource {
        private var source: DataSource? = null
        private val listeners = mutableListOf<TransferListener>()
        override fun addTransferListener(transferListener: TransferListener) { listeners += transferListener; source?.addTransferListener(transferListener) }
        override fun open(dataSpec: DataSpec): Long {
            val selected = if(dataSpec.uri.scheme=="madari-internal") NativeTorrentDataSource() else fallback.createDataSource()
            source=selected; listeners.forEach(selected::addTransferListener)
            return selected.open(dataSpec)
        }
        override fun read(buffer: ByteArray, offset: Int, length: Int) = source?.read(buffer,offset,length) ?: throw IOException("Stream is closed")
        override fun getUri() = source?.uri
        override fun getResponseHeaders(): Map<String,List<String>> = source?.responseHeaders ?: emptyMap()
        override fun close() { source?.close(); source=null }
    }
}
@Composable fun PlayerScreen(playback: Playback, vm: TvViewModel) {
    val context=LocalContext.current
    val scope=rememberCoroutineScope()
    val lifecycle=LocalLifecycleOwner.current.lifecycle.currentStateAsState()
    var position by rememberSaveable(playback.uri) { mutableLongStateOf(playback.resume) }
    var error by remember(playback.uri) { mutableStateOf<String?>(null) }
    var torrentInfo by remember(playback.uri) { mutableStateOf<JSONObject?>(null) }
    LaunchedEffect(playback.uri) {
        if(playback.uri.startsWith("madari-internal://")) while(true) {
            try { torrentInfo=vm.torrentStats(playback.token) }
            catch(e: kotlinx.coroutines.CancellationException) { throw e }
            catch(_: Exception) { torrentInfo=null }
            kotlinx.coroutines.delay(1500)
        }
    }
    var ended by remember(playback.uri) { mutableStateOf(false) }
    var retry by remember { mutableIntStateOf(0) }
    var controlsVisible by remember { mutableStateOf(true) }
    var sourceVideo by remember { mutableStateOf(playback.videoId) }
    var sourceChoices by remember { mutableStateOf<List<Source>>(emptyList()) }
    var sourceLoading by remember { mutableStateOf(false) }
    var sourceError by remember { mutableStateOf<String?>(null) }
    var menu by remember { mutableStateOf<String?>(null) }
    var interaction by remember { mutableIntStateOf(0) }
    var playing by remember { mutableStateOf(true) }
    var duration by remember { mutableLongStateOf(0) }
    var tracks by remember { mutableStateOf(Tracks.EMPTY) }
    var resize by remember { mutableIntStateOf(vm.playerResizeMode()) }
    var speed by remember { mutableFloatStateOf(vm.playerSpeed()) }
    var subtitleSize by remember(playback.uri) { mutableStateOf(vm.playerSubtitleSize()) }
    // The web remote can change source too, so the list is fetched once per playback.
    var allSources by remember(playback.uri) { mutableStateOf<List<Source>>(emptyList()) }
    LaunchedEffect(playback.uri) {
        try { allSources = vm.playerSources(playback.title, playback.videoId) }
        catch(e: kotlinx.coroutines.CancellationException) { throw e }
        catch(_: Exception) { allSources = emptyList() }
    }
    val rootFocus=remember { FocusRequester() }
    val playFocus=remember { FocusRequester() }
    val seekFocus=remember { FocusRequester() }
    var seekFocused by remember { mutableStateOf(false) }
    var seekFeedback by remember { mutableStateOf(false) }
    var seekTick by remember { mutableIntStateOf(0) }
    LaunchedEffect(seekTick) { if(seekFeedback) { delay(1800); seekFeedback=false } }
    val recoveryFocus = remember { FocusRequester() }
    LaunchedEffect(error,ended) { if(error!=null || ended) recoveryFocus.requestFocus() }
    // Release decoder, surface and session when the activity stops. Position survives resume.
    if(!lifecycle.value.isAtLeast(Lifecycle.State.STARTED)) return
    val player = remember(playback.uri,retry) {
        val headerJson = playback.delivery.optJSONObject("request_headers")
        val headers = headerJson?.keys()?.asSequence()?.associateWith { headerJson.getString(it) }.orEmpty()
        val http = DefaultHttpDataSource.Factory().setDefaultRequestProperties(headers).setConnectTimeoutMs(15000).setReadTimeoutMs(30000)
        val factory=PlaybackDataSourceFactory(DefaultDataSource.Factory(context,http))
        val item=MediaItem.Builder().setUri(playback.uri).setMediaId(playback.videoId)
            .setMediaMetadata(MediaMetadata.Builder().setTitle(playback.title.name).build())
        // Per-source credentials must not be forwarded to independent subtitle URLs.
        if(headers.isEmpty()) {
            val subtitles=playback.source.raw.optJSONArray("subtitles").objects().mapNotNull { subtitle ->
                val uri=Uri.parse(subtitle.text("url"))
                if(uri.scheme !in listOf("http","https")) null else MediaItem.SubtitleConfiguration.Builder(uri)
                    .setLanguage(subtitle.text("lang")).setLabel(subtitle.text("lang"))
                    .setMimeType(if(uri.path.orEmpty().endsWith(".vtt",true)) MimeTypes.TEXT_VTT else MimeTypes.APPLICATION_SUBRIP).build()
            }
            item.setSubtitleConfigurations(subtitles)
        }
        ExoPlayer.Builder(context).setMediaSourceFactory(DefaultMediaSourceFactory(factory)).setSeekBackIncrementMs(10000).setSeekForwardIncrementMs(10000).build().apply {
            setAudioAttributes(AudioAttributes.Builder().setUsage(C.USAGE_MEDIA).setContentType(C.AUDIO_CONTENT_TYPE_MOVIE).build(),true)
            setHandleAudioBecomingNoisy(true)
            trackSelectionParameters=trackSelectionParameters.buildUpon().setTrackTypeDisabled(C.TRACK_TYPE_TEXT,!playback.preferences.optBoolean("subtitles_enabled",true)).build()
            setMediaItem(item.build(),position); prepare(); playWhenReady=true
            setPlaybackSpeed(vm.playerSpeed())
        }
    }
    val session=remember(player) { MediaSession.Builder(context,player).setId("madari-${System.nanoTime()}").build() }
    BackHandler {
        if(menu!=null) { menu=null; return@BackHandler }
        if(controlsVisible && error==null && !ended) { controlsVisible=false; return@BackHandler }
        vm.saveProgress(playback,player.currentPosition,player.duration,ended)
        vm.closePlayer()
    }
    DisposableEffect(player) {
        var active = true
        var appliedSignature = ""
        val listener=object : Player.Listener {
            override fun onTracksChanged(currentTracks: Tracks) {
                tracks=currentTracks
                val tracks=currentTracks

                val candidates = mutableListOf<Triple<Int, androidx.media3.common.TrackGroup, Int>>()
                val details = JSONArray()
                tracks.groups.forEachIndexed { groupIndex, group ->
                    if(group.type == C.TRACK_TYPE_AUDIO || group.type == C.TRACK_TYPE_TEXT) {
                        for(index in 0 until group.length) if(group.isTrackSupported(index)) {
                            val format = group.getTrackFormat(index)
                            val id = groupIndex * 10000 + index
                            candidates += Triple(id,group.mediaTrackGroup,index)
                            details.put(obj("id" to id,"kind" to if(group.type==C.TRACK_TYPE_AUDIO) "audio" else "sub",
                                "language" to format.language.orEmpty(),"selected" to group.isTrackSelected(index),
                                "hearing_impaired" to (format.roleFlags and C.ROLE_FLAG_DESCRIBES_MUSIC_AND_SOUND != 0),
                                "visual_impaired" to (format.roleFlags and C.ROLE_FLAG_DESCRIBES_VIDEO != 0),
                                "commentary" to (format.roleFlags and C.ROLE_FLAG_COMMENTARY != 0),
                                "forced" to (format.selectionFlags and C.SELECTION_FLAG_FORCED != 0)))
                        }
                    }
                }
                val signature = candidates.joinToString { "${it.first}:${it.second.id}:${it.second.getFormat(it.third)}" }
                if(signature.isEmpty() || signature==appliedSignature) return
                appliedSignature=signature
                scope.launch {
                    try {
                        val chosen = withContext(Dispatchers.IO) { JSONObject(NativeCore.dispatch("preferred_tracks",obj("tracks" to details,"preferences" to playback.preferences).toString())) }
                        if(active) {
                            val parameters=player.trackSelectionParameters.buildUpon()
                            for(kind in listOf("audio","sub")) if(!chosen.isNull(kind)) {
                                val candidate=candidates.firstOrNull { it.first==chosen.optInt(kind,-1) }
                                if(candidate!=null) parameters.setOverrideForType(TrackSelectionOverride(candidate.second,listOf(candidate.third)))
                            }
                            player.trackSelectionParameters=parameters.build()
                        }
                    } catch(_: java.io.IOException) { /* Keep Media3 defaults if the preference adapter is unavailable. */ }
                }
            }
            override fun onPlayerError(e: PlaybackException) { error=if(playback.uri.startsWith("madari-internal://")) {
                val detail=generateSequence<Throwable>(e) { it.cause }.last().message.orEmpty()
                "Torrent playback interrupted. ${detail.take(220)}"
            } else "This source could not play (${e.errorCodeName}). Retry or choose another source." }
            override fun onPlaybackStateChanged(state: Int) {
                ended=state==Player.STATE_ENDED
                if(ended) vm.saveProgress(playback,player.currentPosition,player.duration,true)
            }
        }
        player.addListener(listener)
        listener.onTracksChanged(player.currentTracks)
        onDispose {
            active=false
            position=player.currentPosition
            vm.saveProgress(playback,position,player.duration,ended)
            player.removeListener(listener); session.release(); player.release()
        }
    }
    /** The whole player state the web remote renders; sent on every tick. */
    fun playerState(): JSONObject {
        val episode=playback.title.videos.firstOrNull { it.text("id")==playback.videoId }
        val trackList=JSONArray()
        tracks.groups.forEachIndexed { groupIndex, group ->
            if(group.type==C.TRACK_TYPE_AUDIO || group.type==C.TRACK_TYPE_TEXT) for(index in 0 until group.length) if(group.isTrackSupported(index)) {
                val format=group.getTrackFormat(index)
                trackList.put(obj("id" to groupIndex*10000+index,"kind" to if(group.type==C.TRACK_TYPE_AUDIO) "audio" else "sub",
                    "label" to format.label.orEmpty(),"language" to format.language.orEmpty(),"codecs" to format.codecs.orEmpty(),
                    "channels" to format.channelCount,"selected" to group.isTrackSelected(index)))
            }
        }
        // Built as JSONArray explicitly: a Kotlin List nested in a JSONObject
        // stringifies to a quoted string on Android, not to a JSON array.
        val sourceList=JSONArray()
        allSources.forEachIndexed { index, entry ->
            sourceList.put(obj("id" to index,"label" to entry.raw.text("name").ifEmpty { entry.name },"current" to (entry.raw.toString()==playback.source.raw.toString())))
        }
        val episodeList=JSONArray()
        playback.title.videos.forEachIndexed { index, entry ->
            episodeList.put(obj("id" to index,"label" to "S${entry.optInt("season")} E${entry.optInt("episode")} · ${entry.text("title")}","current" to (entry.text("id")==playback.videoId)))
        }
        return obj("active" to true,"title" to playback.title.name,
            "episode" to (episode?.let { "Season ${it.optInt("season")} · Episode ${it.optInt("episode")}" } ?: ""),
            "source" to playback.source.name,
            // Episode still first, then the title's poster, so the bar shows the art for what is on.
            "poster" to (episode?.text("thumbnail").orEmpty().ifEmpty { playback.title.poster }),
            "background" to playback.title.background,
            "position_ms" to player.currentPosition,"duration_ms" to duration,"buffered_ms" to player.bufferedPosition,
            "playing" to player.isPlaying,"buffering" to (player.playbackState==Player.STATE_BUFFERING),
            "seekable" to player.isCurrentMediaItemSeekable,"live" to player.isCurrentMediaItemLive,
            "ended" to ended,"error" to (error ?: ""),
            "speed" to speed,"resize" to resize,"subtitle_size" to subtitleSize,
            "tracks" to trackList,"sources" to sourceList,"episodes" to episodeList,
            "torrent" to torrentInfo)
    }
    LaunchedEffect(player) {
        var ticks=0
        while(true) {
            position=player.currentPosition; duration=player.duration.coerceAtLeast(0); playing=player.isPlaying
            if(++ticks%4==0 && player.playbackState==Player.STATE_READY) vm.saveProgress(playback,position,duration)
            vm.publishPlayerState(playerState())
            delay(500)
        }
    }
    LaunchedEffect(controlsVisible,interaction,playing,menu,error,ended) {
        if(controlsVisible && playing && menu==null && error==null && !ended) { delay(5000); controlsVisible=false }
    }
    LaunchedEffect(controlsVisible,menu,error,ended) {
        if(menu==null && error==null && !ended) { if(controlsVisible) playFocus.requestFocus() else rootFocus.requestFocus() }
    }
    val keyHandler by rememberUpdatedState<(KeyEvent)->Boolean> { event ->
        val code=event.keyCode
        val media=code in listOf(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE,KeyEvent.KEYCODE_MEDIA_PLAY,KeyEvent.KEYCODE_MEDIA_PAUSE,KeyEvent.KEYCODE_MEDIA_REWIND,KeyEvent.KEYCODE_MEDIA_FAST_FORWARD)
        val wake=!controlsVisible && menu==null && error==null && !ended && code in listOf(KeyEvent.KEYCODE_DPAD_CENTER,KeyEvent.KEYCODE_ENTER,KeyEvent.KEYCODE_DPAD_UP,KeyEvent.KEYCODE_DPAD_DOWN,KeyEvent.KEYCODE_DPAD_LEFT,KeyEvent.KEYCODE_DPAD_RIGHT)
        if(event.action==KeyEvent.ACTION_DOWN) interaction++
        if((media || wake) && event.action==KeyEvent.ACTION_DOWN) {
            when(code) {
                KeyEvent.KEYCODE_MEDIA_REWIND,KeyEvent.KEYCODE_DPAD_LEFT,
                KeyEvent.KEYCODE_MEDIA_FAST_FORWARD,KeyEvent.KEYCODE_DPAD_RIGHT -> {
                    val forward=code==KeyEvent.KEYCODE_MEDIA_FAST_FORWARD || code==KeyEvent.KEYCODE_DPAD_RIGHT
                    val step=if(event.repeatCount>5) 30000L else 10000L
                    if(player.isCurrentMediaItemSeekable) player.seekTo((player.currentPosition+if(forward) step else -step).coerceIn(0L,player.duration.coerceAtLeast(0)))
                    position=player.currentPosition; seekFeedback=true; seekTick++
                }
                KeyEvent.KEYCODE_DPAD_UP,KeyEvent.KEYCODE_DPAD_DOWN -> controlsVisible=true
                else -> if(event.repeatCount==0) {
                    when(code) {
                        KeyEvent.KEYCODE_MEDIA_PLAY -> player.play()
                        KeyEvent.KEYCODE_MEDIA_PAUSE -> player.pause()
                        else -> if(player.playWhenReady) player.pause() else player.play()
                    }
                    controlsVisible=true
                }
            }
        }
        media || wake
    }
    DisposableEffect(player) {
        val activity=context as? MainActivity
        var consumedKey=-1
        val handler: (KeyEvent)->Boolean = { event ->
            if(event.action==KeyEvent.ACTION_UP && event.keyCode==consumedKey) { consumedKey=-1; true }
            else keyHandler(event).also { if(it && event.action==KeyEvent.ACTION_DOWN) consumedKey=event.keyCode }
        }
        activity?.playerKeyHandler=handler
        onDispose { if(activity?.playerKeyHandler===handler) activity.playerKeyHandler=null }
    }
    LaunchedEffect(menu,sourceVideo) {
        if(menu=="Sources") {
            sourceLoading=true; sourceError=null
            try { sourceChoices=vm.playerSources(playback.title,sourceVideo) }
            catch(e: kotlinx.coroutines.CancellationException) { throw e }
            catch(e: Exception) { sourceError="Sources could not load. Close this panel and try again." }
            finally { sourceLoading=false }
        }
    }
    fun chooseVideo(id: String) { sourceVideo=id;sourceChoices=emptyList();menu="Sources" }
    fun exit() { vm.saveProgress(playback,player.currentPosition,player.duration,ended); vm.closePlayer() }
    // Everything the web remote can ask for, applied against the live player.
    LaunchedEffect(player) {
        vm.playerCommand.collect { command ->
            when(command.optString("action")) {
                "play" -> player.play()
                "pause" -> player.pause()
                "play_pause" -> if(player.playWhenReady) player.pause() else player.play()
                "seek" -> if(player.isCurrentMediaItemSeekable)
                    player.seekTo(command.optLong("position_ms").coerceIn(0L,player.duration.coerceAtLeast(0)))
                "seek_by" -> if(player.isCurrentMediaItemSeekable)
                    player.seekTo((player.currentPosition+command.optLong("offset_ms")).coerceIn(0L,player.duration.coerceAtLeast(0)))
                "speed" -> { val value=command.optDouble("value",1.0).toFloat(); speed=value; player.setPlaybackSpeed(value); vm.setPlayerPreference("player_speed",value) }
                "resize" -> { val value=command.optInt("value",0); resize=value; vm.setPlayerPreference("player_resize",value) }
                "subtitle_size" -> { val value=command.optString("value","medium"); subtitleSize=value; vm.setPlayerPreference("player_subtitle_size",value) }
                "track" -> {
                    val type=if(command.optString("kind")=="audio") C.TRACK_TYPE_AUDIO else C.TRACK_TYPE_TEXT
                    val parameters=player.trackSelectionParameters.buildUpon().clearOverridesOfType(type)
                    if(command.isNull("id") || !command.has("id")) {
                        // Nothing chosen means automatic audio, or subtitles off.
                        parameters.setTrackTypeDisabled(type,type==C.TRACK_TYPE_TEXT)
                    } else {
                        val target=command.optInt("id")
                        parameters.setTrackTypeDisabled(type,false)
                        tracks.groups.forEachIndexed { groupIndex, group ->
                            if(group.type==type) for(index in 0 until group.length) if(group.isTrackSupported(index) && groupIndex*10000+index==target)
                                parameters.setOverrideForType(TrackSelectionOverride(group.mediaTrackGroup,listOf(index)))
                        }
                    }
                    player.trackSelectionParameters=parameters.build()
                }
                "episode" -> playback.title.videos.getOrNull(command.optInt("id",-1))?.let { entry ->
                    val id=entry.text("id"); if(id.isNotEmpty() && id!=playback.videoId) chooseVideo(id)
                }
                "source" -> allSources.getOrNull(command.optInt("id",-1))?.let { entry ->
                    scope.launch {
                        try { vm.replacePlayback(playback,playback.videoId,entry,player.currentPosition,player.duration,ended) }
                        catch(e: kotlinx.coroutines.CancellationException) { throw e }
                        catch(_: Exception) { error="This source could not open. Try another." }
                    }
                }
                "retry" -> { ended=false; error=null; retry++ }
                "stop" -> exit()
            }
            interaction++
        }
    }
    Box(Modifier.fillMaxSize().background(Color.Black).focusRequester(rootFocus).focusable()) {
        AndroidView(factory={ ctx -> PlayerView(ctx).apply {
            layoutParams=ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,ViewGroup.LayoutParams.MATCH_PARENT)
            this.player=player; useController=false
            setShowBuffering(PlayerView.SHOW_BUFFERING_WHEN_PLAYING)
            keepScreenOn=true; isFocusable=false; descendantFocusability=ViewGroup.FOCUS_BLOCK_DESCENDANTS
        } },update={it.player=player;it.resizeMode=resize
            it.subtitleView?.setFractionalTextSize(SubtitleView.DEFAULT_TEXT_SIZE_FRACTION*when(subtitleSize){"small"->.8f;"large"->1.3f;else->1f})
        },modifier=Modifier.fillMaxSize())
        if(controlsVisible && error==null && !ended) {
            Column(Modifier.align(Alignment.TopStart).fillMaxWidth().background(Brush.verticalGradient(listOf(Color.Black.copy(alpha=.85f),Color.Transparent))).padding(36.dp)) {
                Heading(playback.title.name)
                val episode=playback.title.videos.firstOrNull { it.text("id")==playback.videoId }
                Hint(episode?.let { "Season ${it.optInt("season")} · Episode ${it.optInt("episode")}  ${it.text("title")}" } ?: playback.source.name)
            }
            Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth().background(Brush.verticalGradient(listOf(Color.Transparent,Color.Black.copy(alpha=.95f)))).padding(36.dp),verticalArrangement=Arrangement.spacedBy(18.dp)) {
                Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) { Text(timeLabel(position)); Text(if(player.isCurrentMediaItemLive) "LIVE" else timeLabel(duration)) }
                Box(Modifier.fillMaxWidth().height(18.dp).focusRequester(seekFocus).onFocusChanged { seekFocused=it.isFocused }
                    .onPreviewKeyEvent { event ->
                        val code=event.nativeKeyEvent.keyCode
                        if(code==KeyEvent.KEYCODE_DPAD_LEFT || code==KeyEvent.KEYCODE_DPAD_RIGHT) {
                            if(event.nativeKeyEvent.action==KeyEvent.ACTION_DOWN && player.isCurrentMediaItemSeekable) {
                                val step=if(event.nativeKeyEvent.repeatCount>5) 30000L else 10000L
                                player.seekTo((player.currentPosition+if(code==KeyEvent.KEYCODE_DPAD_RIGHT) step else -step).coerceIn(0L,player.duration.coerceAtLeast(0)))
                                interaction++; position=player.currentPosition
                            }; true
                        } else false
                    }.focusProperties { down=playFocus }.focusable().padding(vertical=if(seekFocused) 5.dp else 7.dp).background(Color.White.copy(alpha=.2f))) {
                    Box(Modifier.fillMaxWidth(if(duration>0) (player.bufferedPosition.toFloat()/duration).coerceIn(0f,1f) else 0f).height(4.dp).background(Color.White.copy(alpha=.35f)))
                    Box(Modifier.fillMaxWidth(if(duration>0) (position.toFloat()/duration).coerceIn(0f,1f) else 0f).height(4.dp).background(TvColors.Accent))
                }
                Row(horizontalArrangement=Arrangement.spacedBy(14.dp),verticalAlignment=Alignment.CenterVertically) {
                    Action(if(playing) "Pause" else "Play",{if(player.isPlaying) player.pause() else player.play(); interaction++},Modifier.focusRequester(playFocus).focusProperties { up=seekFocus },icon=if(playing) "pause" else "play",primary=true)
                    Action("10s",{player.seekBack();interaction++},icon="backward")
                    Action("10s",{player.seekForward();interaction++},icon="forward")
                    if(playback.previousVideo!=null) Action("Previous",{chooseVideo(playback.previousVideo)},icon="previous")
                    if(playback.nextVideo!=null) Action("Next",{chooseVideo(playback.nextVideo!!)},icon="next")
                    Spacer(Modifier.weight(1f))
                    Action("Options",{menu="Options"},icon="settings")
                }
            }
        }
        if(seekFeedback && !controlsVisible && error==null) Text(timeLabel(position)+" / "+timeLabel(duration),Modifier.align(Alignment.BottomCenter).padding(bottom=52.dp).background(Color.Black.copy(alpha=.8f)).padding(18.dp),style=MaterialTheme.typography.titleLarge)
        if((controlsVisible || error!=null || player.playbackState==Player.STATE_BUFFERING) && torrentInfo!=null) {
            val info=torrentInfo!!
            val total=info.optLong("total")
            val percent=if(total>0) (info.optLong("downloaded").toDouble()*100/total).toInt().coerceIn(0,100) else 0
            Text("Torrent · ${info.optString("state")} · $percent% · ${info.optInt("peers")} peers · ↓ ${info.optLong("download_speed")/1024} KB/s · ↑ ${info.optLong("upload_speed")/1024} KB/s",
                Modifier.align(Alignment.TopEnd).padding(top=14.dp,end=28.dp).background(Color.Black.copy(alpha=.75f)).padding(10.dp),
                style=MaterialTheme.typography.labelMedium,color=Color.White)
        }
        if(error!=null || ended) Column(Modifier.align(Alignment.Center).width(560.dp).background(TvColors.Panel).padding(28.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
            Heading(if(ended) "You've reached the end" else "Playback interrupted")
            error?.let { Hint(it) }
            Action(if(ended) "Play again" else "Retry",{position=if(ended) 0 else position;error=null;ended=false;retry++},Modifier.focusRequester(recoveryFocus))
            Action("Choose another source",{exit()})
            if(playback.nextVideo != null) Action("Next episode",{chooseVideo(playback.nextVideo!!)})
        }
    }
    menu?.let { page ->
        Dialog(onDismissRequest={menu=null}) {
            val first=remember(page) { FocusRequester() }
            LazyColumn(Modifier.width(550.dp).heightIn(max=460.dp).background(TvColors.Panel).padding(28.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
                item { Heading(page) }
                if(page=="Options") {
                    items(listOf("Audio","Subtitles","Playback speed","Picture size","Episodes","Change source")) { option ->
                        Action(option,{if(option=="Change source") chooseVideo(playback.videoId) else menu=option},Modifier.fillMaxWidth().then(if(option=="Audio") Modifier.focusRequester(first) else Modifier))
                    }
                } else if(page=="Sources") {
                    item { Action("Back",{menu="Options"},Modifier.focusRequester(first)) }
                    if(sourceLoading) item { Hint("Loading sources…") }
                    sourceError?.let { item { Hint(it) } }
                    if(!sourceLoading && sourceChoices.isEmpty() && sourceError==null) item { Hint("No sources available for this episode.") }
                    items(sourceChoices.withIndex().toList(),key={it.index}) { (_,source) ->
                        Action(source.raw.text("name").ifEmpty{source.name}.replace("\n"," · ")+" · "+source.raw.text("title").lineSequence().firstOrNull().orEmpty().take(80),{
                            if(!sourceLoading) scope.launch {
                                sourceLoading=true
                                try { vm.replacePlayback(playback,sourceVideo,source,player.currentPosition,player.duration,ended) }
                                catch(e: kotlinx.coroutines.CancellationException) { throw e }
                                catch(e: Exception) { sourceError="This source could not open. Try another." }
                                finally { sourceLoading=false }
                            }
                        },Modifier.fillMaxWidth(),enabled=!sourceLoading)
                    }
                } else if(page=="Audio" || page=="Subtitles") {
                    val type=if(page=="Audio") C.TRACK_TYPE_AUDIO else C.TRACK_TYPE_TEXT
                    item { Action(if(page=="Audio") "Automatic" else "Off",{player.trackSelectionParameters=player.trackSelectionParameters.buildUpon().clearOverridesOfType(type).setTrackTypeDisabled(type,page=="Subtitles").build();menu=null},Modifier.focusRequester(first)) }
                    tracks.groups.filter { it.type==type }.forEach { group ->
                        items((0 until group.length).filter { group.isTrackSupported(it) }) { index ->
                            val format=group.getTrackFormat(index)
                            val language=format.language?.let { java.util.Locale.forLanguageTag(it).displayLanguage }.orEmpty()
                            Action((if(group.isTrackSelected(index)) "✓ " else "") + listOf(format.label.orEmpty(),language,format.codecs.orEmpty(),if(format.channelCount>0) "${format.channelCount} ch" else "").filter { it.isNotBlank() }.distinct().joinToString(" · ").ifEmpty { "Track ${index+1}" },{
                                player.trackSelectionParameters=player.trackSelectionParameters.buildUpon().setTrackTypeDisabled(type,false).setOverrideForType(TrackSelectionOverride(group.mediaTrackGroup,listOf(index))).build();menu=null
                            },Modifier.fillMaxWidth())
                        }
                    }
                } else if(page=="Playback speed") {
                    items(listOf(.5f,.75f,1f,1.25f,1.5f,2f)) { value -> Action("${if(speed==value) "✓ " else ""}${value}×",{speed=value;player.setPlaybackSpeed(value);vm.setPlayerPreference("player_speed",value);menu=null},if(value==.5f) Modifier.focusRequester(first) else Modifier) }
                } else if(page=="Picture size") {
                    items(listOf("Fit" to AspectRatioFrameLayout.RESIZE_MODE_FIT,"Zoom" to AspectRatioFrameLayout.RESIZE_MODE_ZOOM,"Stretch" to AspectRatioFrameLayout.RESIZE_MODE_FILL)) { (label,value) -> Action(label,{resize=value;vm.setPlayerPreference("player_resize",value);menu=null},if(label=="Fit") Modifier.focusRequester(first) else Modifier) }
                } else if(page=="Episodes") {
                    item { Action("Back",{menu="Options"},Modifier.focusRequester(first)) }
                    if(playback.title.videos.isEmpty()) item { Hint("This title has no episodes.") }
                    items(playback.title.videos,key={it.text("id")}) { episode -> Action("S${episode.optInt("season")} E${episode.optInt("episode")} · ${episode.text("title")}",{chooseVideo(episode.text("id"))},Modifier.fillMaxWidth()) }
                }
            }
            LaunchedEffect(page) { first.requestFocus() }
        }
    }
}
private fun timeLabel(ms: Long): String {
    val seconds=ms.coerceAtLeast(0)/1000
    return if(seconds>=3600) "%d:%02d:%02d".format(seconds/3600,seconds/60%60,seconds%60) else "%d:%02d".format(seconds/60,seconds%60)
}
