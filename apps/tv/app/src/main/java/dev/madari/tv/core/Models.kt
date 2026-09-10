package dev.madari.tv.core

import org.json.JSONObject

/** JSON-backed domain types. Extension fields are preserved on [Title.raw] across JNI. */

data class Title(val provider: String, val raw: JSONObject) {
    val id = raw.text("id")
    val type = raw.text("type")
    val name = raw.text("name").ifEmpty { id }
    val poster = raw.text("poster")
    val background = raw.text("background").ifEmpty { poster }
    val description = raw.text("description")
    val key: JSONObject get() = obj("installation_id" to provider, "content_type" to type, "item_id" to id)
    val identity = "$provider|$type|$id"
    val videos: List<JSONObject> = raw.optJSONArray("videos").objects()
        .sortedWith(compareBy({ it.optInt("season") }, { it.optInt("episode") }))
}

data class Catalog(val provider: String, val providerName: String, val raw: JSONObject) {
    val id = raw.text("id")
    val type = raw.text("type")
    val name = raw.text("name").ifEmpty { id }
    val identity = "$provider|$type|$id"
    val extras = raw.optJSONArray("extra").objects()
    val searchable = extras.any { it.text("name") == "search" } || raw.strings("extraSupported").contains("search")
    val pageable = extras.any { it.text("name") == "skip" } || raw.strings("extraSupported").contains("skip")
    val required = extras.filter { it.optBoolean("isRequired") }.map { it.text("name") } + raw.strings("extraRequired")
}

data class Shelf(
    val id: String,
    val name: String,
    val titles: List<Title>,
    val catalog: Catalog? = null,
    val skip: Int = 0,
    val more: Boolean = false,
    val extras: Map<String, String> = emptyMap()
)

data class Source(val provider: String, val name: String, val raw: JSONObject)

data class Playback(
    val title: Title,
    val videoId: String,
    val source: Source,
    val prepared: JSONObject,
    val nextVideo: String? = null,
    val preferences: JSONObject = JSONObject(),
    val previousVideo: String? = null
) {
    val delivery = prepared.getJSONObject("delivery")
    val token = delivery.optJSONObject("media")?.text("token").orEmpty()
    val uri = if (delivery.text("kind") == "torrent") "madari-internal://$token" else delivery.text("url")
    val resume = prepared.getJSONObject("plan").optLong("resume_ms")
}
