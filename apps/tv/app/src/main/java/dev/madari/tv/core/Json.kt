package dev.madari.tv.core

import org.json.JSONArray
import org.json.JSONObject

/** Null-safe JSON helpers shared by the repository and state layers. */

fun obj(vararg pairs: Pair<String, Any?>): JSONObject =
    JSONObject().apply { pairs.forEach { (key, value) -> put(key, value ?: JSONObject.NULL) } }

fun JSONArray?.objects(): List<JSONObject> =
    if (this == null) emptyList() else (0 until length()).mapNotNull { optJSONObject(it) }

fun JSONObject.text(key: String): String = if (isNull(key)) "" else optString(key)

fun JSONObject.strings(key: String): List<String> =
    optJSONArray(key)?.let { array -> (0 until array.length()).map { array.optString(it) } } ?: emptyList()
