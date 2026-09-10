package dev.madari.tv.state

import dev.madari.tv.core.Catalog
import dev.madari.tv.core.Playback
import dev.madari.tv.core.Shelf
import dev.madari.tv.core.Source
import dev.madari.tv.core.Title
import org.json.JSONObject

/** Immutable snapshot rendered by every TV screen. */
data class TvState(
    val loading: Boolean = true,
    val resumingTitle: String? = null,
    val error: String? = null,
    val profiles: List<JSONObject> = emptyList(),
    val profileAvatars: List<JSONObject> = emptyList(),
    val activeKids: String = "",
    val profile: JSONObject? = null,
    val snapshot: JSONObject = JSONObject(),
    val tab: String = "Home",
    val shelves: List<Shelf> = emptyList(),
    val detail: Title? = null,
    val videoId: String? = null,
    val sources: List<Source>? = null,
    val playback: Playback? = null,
    val settingsUnlocked: Boolean = false,
    val web: JSONObject = JSONObject(),
    val webAddress: String = "",
    val calendar: JSONObject = JSONObject(),
    val query: String = "",
    val notices: List<String> = emptyList(),
    val catalog: Catalog? = null,
    val trakt: JSONObject = JSONObject(),
    val traktDevice: JSONObject = JSONObject()
)
