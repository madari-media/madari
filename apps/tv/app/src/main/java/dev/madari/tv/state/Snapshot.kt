package dev.madari.tv.state

import dev.madari.tv.core.Title
import dev.madari.tv.core.obj
import dev.madari.tv.core.objects
import dev.madari.tv.core.text
import org.json.JSONObject

/** Derives screen models from the native snapshot (library, progress, hidden-continue). */

fun sameKey(a: JSONObject?, b: JSONObject): Boolean =
    a != null && listOf("installation_id", "content_type", "item_id").all { a.text(it) == b.text(it) }

fun savedTitles(snapshot: JSONObject): List<Title> = snapshot.optJSONArray("library").objects().map { entry ->
    val key = entry.getJSONObject("key")
    Title(
        key.text("installation_id"),
        entry.optJSONObject("metadata")
            ?: obj("id" to key.text("item_id"), "type" to key.text("content_type"), "name" to entry.text("title"))
    )
}

/** One Continue watching card: cached full metadata plus the next video chosen by the core. */
data class ContinueEntry(
    val title: Title,
    val meta: JSONObject,
    val hasVideos: Boolean,
    val episode: JSONObject?
)

fun continueTitles(snapshot: JSONObject): List<Title> {
    val hidden = snapshot.optJSONArray("hidden_continue").objects()
    return snapshot.optJSONArray("progress").objects().asReversed()
        .filter { progress ->
            (!progress.optBoolean("completed") || progress.getJSONObject("key").text("content_type") == "series") &&
                progress.optLong("position_ms") > 0 &&
                hidden.none { sameKey(it, progress.getJSONObject("key")) }
        }
        .map { entry ->
            val key = entry.getJSONObject("key")
            Title(
                key.text("installation_id"),
                entry.optJSONObject("metadata")
                    ?: obj("id" to key.text("item_id"), "type" to key.text("content_type"), "name" to key.text("item_id"))
            )
        }
        .distinctBy { it.identity }
}
