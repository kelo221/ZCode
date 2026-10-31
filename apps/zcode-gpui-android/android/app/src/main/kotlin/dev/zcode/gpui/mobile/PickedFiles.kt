package dev.zcode.gpui.mobile

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Log

/**
 * Reads files the user picked (or the keyboard inserted) and hands their
 * bytes to Rust. The phone is a remote client, so a phone path would mean
 * nothing to the desktop agent; the bytes are uploaded to the desktop.
 */
object PickedFiles {
    private const val TAG = "zcode_gpui"
    /** Matches the 20 MiB attachment limit on the Rust side. */
    private const val MAX_BYTES = 20 * 1024 * 1024

    /** Read every uri off the UI thread; `done` runs after the last one. */
    fun deliver(context: Context, uris: List<Uri>, done: () -> Unit = {}) {
        Thread {
            try {
                for (uri in uris) deliverOne(context, uri)
            } finally {
                done()
            }
        }.start()
    }

    private fun deliverOne(context: Context, uri: Uri) {
        val resolver = context.contentResolver
        var name = uri.lastPathSegment ?: "attachment"
        var size = -1L
        runCatching {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)
                ?.use { c ->
                    if (c.moveToFirst()) {
                        c.getString(0)?.let { name = it }
                        if (!c.isNull(1)) size = c.getLong(1)
                    }
                }
        }
        if (size > MAX_BYTES) {
            NativeBridge.nativeAttachError("$name is larger than 20 MiB")
            return
        }
        val mime = resolver.getType(uri) ?: "application/octet-stream"
        val bytes = runCatching {
            resolver.openInputStream(uri)?.use { input ->
                val out = java.io.ByteArrayOutputStream()
                val buf = ByteArray(64 * 1024)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    if (out.size() + n > MAX_BYTES) return@use null
                    out.write(buf, 0, n)
                }
                out.toByteArray()
            }
        }.onFailure { Log.w(TAG, "reading $uri failed: $it") }.getOrNull()
        if (bytes == null) {
            NativeBridge.nativeAttachError("Could not read $name (it may be larger than 20 MiB)")
            return
        }
        NativeBridge.nativeAttachFile(name, mime, bytes)
    }
}
