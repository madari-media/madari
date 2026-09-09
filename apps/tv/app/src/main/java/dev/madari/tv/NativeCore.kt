package dev.madari.tv

import android.app.Application
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

/** JNI owns the shared Rust core, profile sessions, SQLite and torrent engine. */
object NativeCore {
    init { System.loadLibrary("madari_tv") }
    @JvmStatic external fun initializeTls(context: android.content.Context)
    @JvmStatic external fun initialize(path: String)
    @JvmStatic external fun dispatch(operation: String, arguments: String): String
    @JvmStatic external fun openMedia(uri: String, position: Long): Long
    @JvmStatic external fun mediaLength(handle: Long): Long
    @JvmStatic external fun readMedia(handle: Long, buffer: ByteArray, offset: Int, length: Int): Int
    @JvmStatic external fun closeMedia(handle: Long)
}
class MadariApplication : Application(), coil.ImageLoaderFactory {
    override fun newImageLoader(): coil.ImageLoader = coil.ImageLoader.Builder(this)
        .components { add(coil.decode.SvgDecoder.Factory()) }
        .crossfade(160)
        .build()
}

fun obj(vararg pairs: Pair<String, Any?>) = JSONObject().apply { pairs.forEach { (k,v) -> put(k, v ?: JSONObject.NULL) } }
fun JSONArray?.objects(): List<JSONObject> = if (this == null) emptyList() else (0 until length()).mapNotNull { optJSONObject(it) }
fun JSONObject.text(key: String): String = if (isNull(key)) "" else optString(key)
fun JSONObject.strings(key: String): List<String> = optJSONArray(key)?.let { a -> (0 until a.length()).map { a.optString(it) } } ?: emptyList()

class CoreRepository(private val application: Application) {
    suspend fun initialize() = withContext(Dispatchers.IO) { NativeCore.initializeTls(application); NativeCore.initialize(application.filesDir.resolve("madari").absolutePath) }
    suspend fun call(operation: String, arguments: Any = JSONObject()): String = withContext(Dispatchers.IO) { NativeCore.dispatch(operation, arguments.toString()) }
    suspend fun objectCall(operation: String, arguments: Any = JSONObject()) = withContext(Dispatchers.IO) { JSONObject(NativeCore.dispatch(operation, arguments.toString())) }
}

data class Title(val provider: String, val raw: JSONObject) {
    val id = raw.text("id")
    val type = raw.text("type")
    val name = raw.text("name").ifEmpty { id }
    val poster = raw.text("poster")
    val background = raw.text("background").ifEmpty { poster }
    val description = raw.text("description")
    val key: JSONObject get() = obj("installation_id" to provider, "content_type" to type, "item_id" to id)
    val identity = "$provider|$type|$id"
    val videos: List<JSONObject> = raw.optJSONArray("videos").objects().sortedWith(compareBy({ it.optInt("season") }, { it.optInt("episode") }))
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
data class Shelf(val id: String, val name: String, val titles: List<Title>, val catalog: Catalog? = null, val skip: Int = 0, val more: Boolean = false, val extras: Map<String,String> = emptyMap())
data class Source(val provider: String, val name: String, val raw: JSONObject)
data class Playback(val title: Title, val videoId: String, val source: Source, val prepared: JSONObject, val nextVideo: String? = null, val preferences: JSONObject = JSONObject(), val previousVideo: String? = null) {
    val delivery = prepared.getJSONObject("delivery")
    val token = delivery.optJSONObject("media")?.text("token").orEmpty()
    val uri = if (delivery.text("kind") == "torrent") "madari-internal://$token" else delivery.text("url")
    val resume = prepared.getJSONObject("plan").optLong("resume_ms")
}
