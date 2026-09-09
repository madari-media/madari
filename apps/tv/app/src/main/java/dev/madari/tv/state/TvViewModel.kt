package dev.madari.tv.state

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.madari.tv.core.Catalog
import dev.madari.tv.core.CoreRepository
import dev.madari.tv.core.Playback
import dev.madari.tv.core.Shelf
import dev.madari.tv.core.Source
import dev.madari.tv.core.Title
import dev.madari.tv.core.obj
import dev.madari.tv.core.objects
import dev.madari.tv.core.text
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import org.json.JSONArray
import org.json.JSONObject

// The ViewModel orchestrates screens. Source validation, resume policy and authorization
// stay in madari-core / madari-native; JSON preserves extension fields across JNI.
class TvViewModel(application: Application) : AndroidViewModel(application) {
    private val repository = CoreRepository(application)
    private val mutable = MutableStateFlow(TvState())
    val state = mutable.asStateFlow()
    private var operation: Job? = null
    private var generation = 0
    private var cachedHome = emptyList<Shelf>()
    init {
        run { repository.initialize(); loadProfiles(); startWeb() }
        viewModelScope.launch {
            var revision = 0L
            while (true) {
                delay(3000)
                try {
                    val status = repository.objectCall("web_status")
                    if(status.toString()!=state.value.web.toString()) {
                        val address=webAddress(status)
                        mutable.update { it.copy(web = status, webAddress = address) }
                    }
                    val next = status.optLong("revision")
                    if (next != revision && !state.value.loading && state.value.playback == null) {
                        revision = next
                        run {
                            loadProfiles()
                            if (state.value.profile != null) {
                                val current = state.value.profiles.firstOrNull { it.text("id") == state.value.profile?.text("id") }
                                mutable.update { it.copy(profile = current ?: it.profile) }
                                refreshSnapshot()
                                if (state.value.tab == "Home") loadHome()
                            }
                        }
                    }
                } catch (e: CancellationException) { throw e } catch (_: Exception) { /* Initial native startup or server unavailable. */ }
            }
        }
    }
    private fun webAddress(status: JSONObject): String {
        if (!status.optBoolean("running")) return ""
        val connectivity = getApplication<Application>().getSystemService(android.content.Context.CONNECTIVITY_SERVICE) as android.net.ConnectivityManager
        val ip = connectivity.getLinkProperties(connectivity.activeNetwork)?.linkAddresses?.map { it.address }?.firstOrNull { it is java.net.Inet4Address && !it.isLoopbackAddress }?.hostAddress
        return if(ip != null) "http://$ip:${status.optInt("port",11471)}" else "Connect this TV to Wi-Fi or Ethernet"
    }
    private suspend fun startWeb() {
        try { val status = repository.objectCall("web_start"); mutable.update { it.copy(web = status, webAddress = webAddress(status)) } }
        catch(e: Exception) { mutable.update { it.copy(web = obj("running" to false), webAddress = e.message.orEmpty()) } }
    }
    fun toggleWeb() = run {
        if(state.value.web.optBoolean("running")) { val status = repository.objectCall("web_stop"); mutable.update { it.copy(web = status, webAddress = "") } }
        else startWeb()
    }
    private fun run(replace: Boolean = false, block: suspend () -> Unit) {
        if (operation?.isActive == true && !replace) return
        if (replace) operation?.cancel()
        val version = ++generation
        operation = viewModelScope.launch {
            mutable.update { it.copy(loading = true, error = null) }
            try { block() } catch (e: CancellationException) { throw e }
            catch (e: Exception) { mutable.update { it.copy(error = e.message ?: "Operation failed. Please retry.") } }
            finally { if(version == generation) mutable.update { it.copy(loading = false) } }
        }
    }
    fun dismissError() { mutable.update { it.copy(error = null) } }
    private suspend fun loadProfiles() {
        val result = repository.objectCall("profiles")
        mutable.update { it.copy(profiles = result.optJSONArray("profiles").objects(), activeKids = result.optJSONObject("active_kids")?.text("id").orEmpty()) }
    }
    fun createProfile(name: String, pin: String, kids: Boolean = false) = run {
        repository.call("create_profile", obj("name" to name, "pin" to pin, "kids" to kids)); loadProfiles()
    }
    /**
     * Create a profile from the picker. The native core requires an authorized regular
     * profile when profiles already exist, so unlock the chosen adult, authorize it,
     * create the profile and always close the temporary session afterwards.
     */
    fun addProfile(adult: JSONObject, adultPin: String, name: String, pin: String, kids: Boolean) = run {
        try {
            repository.objectCall("unlock", obj("id" to adult.text("id"), "pin" to adultPin))
            repository.call("authorize", obj("pin" to adultPin))
            repository.call("create_profile", obj("name" to name, "pin" to pin, "kids" to kids))
        } finally {
            // Never leave the picker holding another profile's session.
            try { repository.call("leave", obj("pin" to adultPin)) }
            catch (e: CancellationException) { throw e }
            catch (_: Exception) { /* Already gone. */ }
        }
        loadProfiles()
    }
    fun unlock(profile: JSONObject, pin: String) = run {
        cachedHome = emptyList()
        val selected = repository.objectCall("unlock", obj("id" to profile.text("id"), "pin" to pin))
        mutable.update { TvState(loading = true, profile = selected, web = it.web, webAddress = it.webAddress) }
        refreshSnapshot(); loadHome()
    }
    fun leave(pin: String) = run {
        repository.call("leave", obj("pin" to pin))
        mutable.value = TvState(loading = true, web = state.value.web, webAddress = state.value.webAddress)
        loadProfiles()
    }
    private suspend fun refreshSnapshot() { val snapshot = repository.objectCall("snapshot"); mutable.update { it.copy(snapshot = snapshot) } }
    fun catalogs(): List<Catalog> = state.value.snapshot.optJSONArray("addons").objects().filter { it.optBoolean("enabled") }.flatMap { addon ->
        val manifest = addon.getJSONObject("manifest")
        manifest.optJSONArray("catalogs").objects().map { Catalog(addon.text("installation_id"), manifest.text("name"), it) }
    }
    private suspend fun fetch(catalog: Catalog, extras: Map<String,String> = emptyMap(), skip: Int = 0): Shelf {
        val fields = extras.toMutableMap().apply { if (catalog.pageable && skip > 0) put("skip",skip.toString()) }
        val result = repository.objectCall("query", obj("installation_id" to catalog.provider, "request" to obj("resource" to "catalog", "type" to catalog.type, "id" to catalog.id, "extra" to JSONObject(fields))))
        val titles = result.optJSONArray("data").objects().map { Title(catalog.provider,it) }.distinctBy { it.identity }
        return Shelf(catalog.identity, catalog.name, titles, catalog, skip, catalog.pageable && titles.size >= 100, extras)
    }
    private suspend fun loadHome() = coroutineScope {
        val declared = catalogs().filter { it.required.isEmpty() }
        val rows = arrayOfNulls<Shelf>(declared.size)
        val failures = mutableListOf<String>()
        val permits = Semaphore(3)
        declared.mapIndexed { index, catalog -> async {
            permits.withPermit {
                try { rows[index] = fetch(catalog) }
                catch (e: CancellationException) { throw e }
                catch (_: Exception) { failures += "${catalog.providerName} · ${catalog.name} could not load" }
                cachedHome = rows.filterNotNull()
                mutable.update { it.copy(shelves = cachedHome, notices = failures.toList()) }
            }
        } }.awaitAll()
    }
    fun selectTab(tab: String) = run(replace = true) {
        if (state.value.settingsUnlocked) repository.call("lock_settings")
        mutable.update { it.copy(tab = tab, detail = null, sources = null, catalog = null, settingsUnlocked = false, notices = emptyList(), shelves = if(tab == "Home") cachedHome else emptyList(), query = "") }
        refreshSnapshot()
        if (tab == "Home" && cachedHome.isEmpty()) loadHome()
        if (tab == "Calendar") { val calendar = repository.objectCall("calendar"); mutable.update { it.copy(calendar = calendar) } }
    }
    fun refresh() = run { refreshSnapshot(); if (state.value.tab == "Home") loadHome() }
    fun search(query: String) = run(replace = true) {
        mutable.update { it.copy(query = query, shelves = emptyList(), notices = emptyList()) }
        if (query.isBlank()) return@run
        val rows = mutableListOf<Shelf>(); val errors = mutableListOf<String>()
        for (catalog in catalogs().filter { it.searchable && it.required.all { field -> field == "search" } }) {
            try { rows += fetch(catalog,mapOf("search" to query.trim())) }
            catch (e: CancellationException) { throw e }
            catch (_: Exception) { errors += "${catalog.providerName} search failed" }
            mutable.update { it.copy(shelves = rows.toList(), notices = errors.toList()) }
        }
    }
    fun openCatalog(catalog: Catalog, extras: Map<String,String>) = run {
        mutable.update { it.copy(catalog = catalog, shelves = emptyList()) }
        val shelf = fetch(catalog, extras)
        mutable.update { it.copy(shelves = listOf(shelf)) }
    }
    fun loadMore(shelf: Shelf) = run {
        val catalog = shelf.catalog ?: return@run
        val next = fetch(catalog, shelf.extras, shelf.skip + 100)
        val merged = (shelf.titles + next.titles).distinctBy { it.identity }
        val updated = shelf.copy(titles = merged, skip = next.skip, more = next.more && merged.size > shelf.titles.size)
        mutable.update { s -> s.copy(shelves = s.shelves.map { if (it.id == shelf.id) updated else it }) }
    }
    fun open(title: Title) = run(replace = true) {
        mutable.update { it.copy(detail = title, sources = null, videoId = null) }
        val metadata = repository.objectCall("metadata", obj("key" to title.key, "preview" to title.raw))
        val episode = repository.objectCall("episode",obj("meta" to metadata,"key" to title.key,"today" to java.time.LocalDate.now().toString()))
        mutable.update { it.copy(detail = Title(title.provider,metadata), videoId = episode.optJSONObject("video")?.text("id")) }
    }
    private suspend fun continueVideo(title: Title): JSONObject? = repository.objectCall("episode",obj("meta" to title.raw,"key" to title.key,"today" to java.time.LocalDate.now().toString())).optJSONObject("video")
    /**
     * Resolve continue-watching metadata in one cached batch (`continue_metadata`) and ask the
     * core which video each series should resume. Movies need no episode lookup.
     */
    suspend fun continueEntries(titles: List<Title>): List<ContinueEntry> {
        if (titles.isEmpty()) return emptyList()
        val keys = JSONArray().apply { titles.forEach { put(it.key) } }
        val resolved = try { JSONArray(repository.call("continue_metadata", keys)) }
        catch (e: CancellationException) { throw e }
        catch (_: Exception) { JSONArray() }
        val metas = HashMap<String, JSONObject>()
        for (index in 0 until resolved.length()) {
            val pair = resolved.optJSONArray(index) ?: continue
            val key = pair.optJSONObject(0) ?: continue
            val meta = pair.optJSONObject(1)?.optJSONObject("meta") ?: continue
            metas["${key.text("installation_id")}|${key.text("content_type")}|${key.text("item_id")}"] = meta
        }
        return titles.map { title ->
            val meta = metas[title.identity] ?: title.raw
            val hasVideos = (meta.optJSONArray("videos")?.length() ?: 0) > 0
            val episode = if (title.type == "series" && hasVideos) try {
                repository.objectCall("episode", obj("meta" to meta, "key" to title.key, "today" to java.time.LocalDate.now().toString())).optJSONObject("video")
            } catch (e: CancellationException) { throw e } catch (_: Exception) { null } else null
            ContinueEntry(title, meta, hasVideos, episode)
        }
    }
    fun resumeContinue(title: Title, knownVideoId: String? = null) = run(replace = true) {
        mutable.update { it.copy(resumingTitle=title.identity) }
        try {
        val metadata=repository.objectCall("metadata",obj("key" to title.key,"preview" to title.raw))
        val full=Title(title.provider,metadata)
        val history=state.value.snapshot.optJSONArray("progress").objects().filter { sameKey(it.optJSONObject("key"),title.key) }
        val episode=knownVideoId ?: if(full.type=="series") continueVideo(full)?.text("id") else null
        if(full.type=="series" && full.videos.isNotEmpty() && episode==null) {
            // Nothing left to continue: show the title instead of failing the action.
            mutable.update { it.copy(detail=full,videoId=null,sources=null,notices=emptyList()) }
            return@run
        }
        val video=episode ?: history.lastOrNull()?.text("video_id")?.takeIf { it.isNotBlank() }
            ?: metadata.optJSONObject("behaviorHints")?.text("defaultVideoId")?.takeIf { it.isNotBlank() } ?: title.id
        val sources=playerSources(full,video)
        val saved=history.lastOrNull { it.text("video_id")==video } ?: history.lastOrNull()
        val group=saved?.text("binge_group").orEmpty()
        if(group.isNotBlank()) {
            val matches=sources.filter { it.raw.optJSONObject("behaviorHints")?.text("bingeGroup")==group }
                .sortedBy { it.provider!=saved?.text("source_provider") }
            for(source in matches.take(3)) {
                try {
                    prepare(full,video,source)
                    mutable.update { it.copy(detail=full,videoId=video,sources=null,notices=emptyList()) }
                    return@run
                }
                catch(e: kotlinx.coroutines.CancellationException) { throw e }
                catch(_: Exception) { /* Keep the fetched source picker as fallback. */ }
            }
        }
        // Fall back to manual source selection when automatic resume is unavailable.
        mutable.update { it.copy(detail=full,videoId=video,sources=sources,notices=emptyList()) }
        } finally {
            mutable.update { if(it.resumingTitle==title.identity) it.copy(resumingTitle=null) else it }
        }
    }
    fun isSaved(title: Title) = state.value.snapshot.optJSONArray("library").objects().any { sameKey(it.optJSONObject("key"), title.key) }
    fun toggleSaved(title: Title) = run {
        if (isSaved(title)) repository.call("remove", title.key)
        else repository.call("save", obj("key" to title.key,"title" to title.name,"metadata" to title.raw))
        refreshSnapshot()
    }
    fun hideContinue(title: Title) = run { repository.call("hide_continue",obj("key" to title.key,"hidden" to true)); refreshSnapshot() }
    fun sources(title: Title, videoId: String) = run(replace = true) { fetchSources(title,videoId) }
    private suspend fun fetchSources(title: Title, videoId: String) {
        mutable.update { it.copy(videoId = videoId, sources = emptyList(), notices = emptyList()) }
        val result = JSONArray(repository.call("query_all",obj("resource" to "stream","type" to title.type,"id" to videoId)))
        val names = state.value.snapshot.optJSONArray("addons").objects().associate { it.text("installation_id") to it.getJSONObject("manifest").text("name") }
        val sources = mutableListOf<Source>(); val errors = mutableListOf<String>()
        for (provider in result.objects()) {
            val id = provider.text("installation_id"); val name = names[id] ?: "Addon"
            val output = provider.getJSONObject("result")
            if (output.has("Err")) errors += "$name could not return sources"
            output.optJSONObject("Ok")?.optJSONArray("data").objects().forEach { sources += Source(id,name,it) }
        }
        mutable.update { it.copy(sources = sources, notices = errors) }
    }
    suspend fun torrentStats(token: String) = repository.objectCall("torrent_stats",obj("token" to token))
    fun play(title: Title, videoId: String, source: Source) = run { prepare(title,videoId,source) }
    private suspend fun prepare(title: Title, videoId: String, source: Source) {
        val result = repository.objectCall("prepare",obj("source" to source.raw,"key" to title.key,"video_id" to videoId,
            "capabilities" to obj("torrent" to true,"request_headers" to true,"url_schemes" to JSONArray(listOf("http","https")))))
        val kind = result.getJSONObject("delivery").text("kind")
        if (kind != "direct" && kind != "torrent") error("This source cannot play on this TV. Choose another source.")
        repository.call("hide_continue",obj("key" to title.key,"hidden" to false))
        val episode = repository.objectCall("episode",obj("meta" to title.raw,"key" to title.key,"current" to videoId,"today" to java.time.LocalDate.now().toString()))
        val previous = repository.objectCall("episode",obj("meta" to title.raw,"key" to title.key,"current" to videoId,"previous" to true,"today" to java.time.LocalDate.now().toString()))
        mutable.update { it.copy(playback = Playback(title,videoId,source,result,episode.optJSONObject("video")?.text("id"),state.value.snapshot.optJSONObject("playback_preferences") ?: JSONObject(),previous.optJSONObject("video")?.text("id"))) }
    }
    suspend fun playerSources(title: Title, videoId: String): List<Source> {
        val result = JSONArray(repository.call("query_all",obj("resource" to "stream","type" to title.type,"id" to videoId)))
        val names = state.value.snapshot.optJSONArray("addons").objects().associate { it.text("installation_id") to it.getJSONObject("manifest").text("name") }
        return result.objects().flatMap { provider ->
            val id = provider.text("installation_id")
            provider.getJSONObject("result").optJSONObject("Ok")?.optJSONArray("data").objects().map { Source(id,names[id] ?: "Addon",it) }
        }
    }
    suspend fun replacePlayback(old: Playback, videoId: String, source: Source, position: Long, duration: Long, completed: Boolean = false) {
        if(duration > 0) persistProgress(old,position.coerceIn(0,duration),duration,completed)
        refreshSnapshot()
        prepare(old.title,videoId,source)
        if(old.token.isNotEmpty()) repository.call("revoke",obj("token" to old.token))
    }
    private suspend fun persistProgress(playback: Playback, position: Long, duration: Long, completed: Boolean) {
        repository.call("progress",obj("key" to playback.title.key,"metadata" to playback.title.raw,"video_id" to playback.videoId,
            "position_ms" to position.coerceIn(0,duration),"duration_ms" to duration,"completed" to completed,
            "source_provider" to playback.source.provider,"binge_group" to playback.source.raw.optJSONObject("behaviorHints")?.text("bingeGroup")?.takeIf { it.isNotEmpty() }))
    }
    fun saveProgress(playback: Playback, position: Long, duration: Long, completed: Boolean = false) {
        if (position < 0 || duration <= 0) return
        viewModelScope.launch {
            try { repository.call("progress",obj("key" to playback.title.key,"metadata" to playback.title.raw,"video_id" to playback.videoId,"position_ms" to position.coerceAtMost(duration),
                "duration_ms" to duration,"completed" to completed,"source_provider" to playback.source.provider,
                "binge_group" to playback.source.raw.optJSONObject("behaviorHints")?.text("bingeGroup")?.takeIf { it.isNotEmpty() })) }
            catch (e: CancellationException) { throw e }
            catch (_: Exception) { mutable.update { it.copy(error = "Playback position could not be saved.") } }
        }
    }
    fun closePlayer(next: Boolean = false) {
        val playback = state.value.playback ?: return
        mutable.update { it.copy(playback = null) }
        run {
            if (playback.token.isNotEmpty()) repository.call("revoke", obj("token" to playback.token))
            refreshSnapshot()
            if(next && playback.nextVideo != null) fetchSources(playback.title,playback.nextVideo)
        }
    }
    fun authorize(pin: String) = run { repository.call("authorize",obj("pin" to pin)); loadProfiles(); mutable.update { it.copy(settingsUnlocked = true) } }
    fun install(url: String, allowLocal: Boolean) = run { repository.call("install",obj("url" to url.trim(),"allow_local" to allowLocal)); refreshSnapshot() }
    fun enable(addon: JSONObject) = run { repository.call("enable",obj("id" to addon.text("installation_id"),"enabled" to !addon.optBoolean("enabled"))); refreshSnapshot() }
    fun removeAddon(addon: JSONObject) = run { repository.call("remove_addon",obj("id" to addon.text("installation_id"))); refreshSnapshot() }
    fun moveAddon(addon: JSONObject, delta: Int) = run {
        val ids = state.value.snapshot.optJSONArray("addons").objects().map { it.text("installation_id") }.toMutableList()
        val index = ids.indexOf(addon.text("installation_id")); val target = index + delta
        if (index >= 0 && target in ids.indices) { java.util.Collections.swap(ids,index,target); repository.call("reorder",JSONArray(ids)); refreshSnapshot() }
    }
    fun back() {
        val cancellingResume=state.value.resumingTitle!=null
        operation?.cancel()
        generation++
        mutable.update { it.copy(loading = false, error = null, resumingTitle=null) }
        if(cancellingResume) return
        when {
            state.value.sources != null -> mutable.update { it.copy(sources = null, notices = emptyList()) }
            state.value.detail != null -> mutable.update { it.copy(detail = null) }
            state.value.catalog != null -> mutable.update { it.copy(catalog = null) }
            state.value.tab != "Home" -> selectTab("Home")
        }
    }
}
