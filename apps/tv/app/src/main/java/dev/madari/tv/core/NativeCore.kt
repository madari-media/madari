package dev.madari.tv.core

import android.app.Application
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
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

/** Bridges Kotlin to the native core on the IO dispatcher. Every call crosses JNI as JSON. */
class CoreRepository(private val application: Application) {
    suspend fun initialize() = withContext(Dispatchers.IO) {
        NativeCore.initializeTls(application)
        NativeCore.initialize(application.filesDir.resolve("madari").absolutePath)
    }
    suspend fun call(operation: String, arguments: Any = JSONObject()): String =
        withContext(Dispatchers.IO) { NativeCore.dispatch(operation, arguments.toString()) }
    suspend fun objectCall(operation: String, arguments: Any = JSONObject()) =
        withContext(Dispatchers.IO) { JSONObject(NativeCore.dispatch(operation, arguments.toString())) }
}
