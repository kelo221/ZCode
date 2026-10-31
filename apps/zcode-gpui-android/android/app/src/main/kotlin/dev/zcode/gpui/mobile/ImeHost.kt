package dev.zcode.gpui.mobile

import android.content.Context
import android.os.Bundle
import android.text.InputType
import android.text.TextUtils
import android.util.Log
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.CompletionInfo
import android.view.inputmethod.CorrectionInfo
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputContentInfo
import android.view.inputmethod.SurroundingText
import android.view.inputmethod.TextAttribute

private const val TAG = "zcode_ime"

/** Rust-side mirror of the focused text field (UTF-16 offsets, absolute). */
class ImeMirror {
    @Volatile var text: String = ""
    @Volatile var windowStart: Int = 0
    @Volatile var selStart: Int = 0
    @Volatile var selEnd: Int = 0
    @Volatile var compStart: Int = -1
    @Volatile var compEnd: Int = -1

    private fun local(abs: Int) = (abs - windowStart).coerceIn(0, text.length)

    fun before(n: Int): CharSequence {
        val end = local(minOf(selStart, selEnd))
        return text.substring(maxOf(0, end - n), end)
    }

    fun after(n: Int): CharSequence {
        val start = local(maxOf(selStart, selEnd))
        return text.substring(start, minOf(text.length, start + n))
    }

    fun surrounding(before: Int, after: Int): SurroundingText {
        val selLo = local(minOf(selStart, selEnd))
        val selHi = local(maxOf(selStart, selEnd))
        val from = maxOf(0, selLo - maxOf(0, before))
        val to = minOf(text.length, selHi + maxOf(0, after))
        return SurroundingText(text.substring(from, to), selLo - from, selHi - from, windowStart + from)
    }

    fun selected(): CharSequence? {
        if (selStart == selEnd) return null
        return text.substring(local(minOf(selStart, selEnd)), local(maxOf(selStart, selEnd)))
    }
}

/**
 * Invisible 1x1 editor view that owns the IME connection. GameActivity's
 * SurfaceView keeps rendering and touch; text input flows from here to
 * gpui's focused input handler through the Rust bridge.
 */
class ImeHostView(context: Context, private val mirror: ImeMirror) : View(context) {
    /** 0 = terminal-like (no suggestions), 1 = text editor. */
    @Volatile var mode: Int = 1

    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    override fun onCheckIsTextEditor(): Boolean = true

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection {
        outAttrs.inputType = if (mode == 0) {
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD
        } else {
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or
                InputType.TYPE_TEXT_FLAG_CAP_SENTENCES or InputType.TYPE_TEXT_FLAG_AUTO_CORRECT
        }
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_EXTRACT_UI or
            EditorInfo.IME_FLAG_NO_FULLSCREEN or EditorInfo.IME_ACTION_NONE
        // Lets Gboard's clipboard and image keyboards insert images.
        if (mode == 1) outAttrs.contentMimeTypes = arrayOf("image/*")
        outAttrs.initialSelStart = mirror.selStart
        outAttrs.initialSelEnd = mirror.selEnd
        return Connection(this, mirror)
    }

    private class Connection(private val view: View, private val mirror: ImeMirror) :
        BaseInputConnection(view, false) {

        override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean {
            NativeBridge.nativeImeCommitText(0, text?.toString() ?: "", newCursorPosition)
            return true
        }

        override fun setComposingText(text: CharSequence?, newCursorPosition: Int): Boolean {
            NativeBridge.nativeImeSetComposingText(0, text?.toString() ?: "", newCursorPosition)
            return true
        }

        // Gboard swaps a typed word for a tapped suggestion by marking it
        // (setComposingRegion) or replacing it (replaceText). The Base defaults
        // act on getEditable(), which is null for this view, so they did nothing.
        override fun setComposingRegion(start: Int, end: Int): Boolean {
            Log.d(TAG, "setComposingRegion $start..$end")
            NativeBridge.nativeImeSetComposingRegion(0, start, end)
            return true
        }

        override fun replaceText(
            start: Int,
            end: Int,
            text: CharSequence,
            newCursorPosition: Int,
            textAttribute: TextAttribute?,
        ): Boolean {
            Log.d(TAG, "replaceText $start..$end ${text.length}")
            NativeBridge.nativeImeReplaceText(0, start, end, text.toString())
            return true
        }

        override fun getSurroundingText(
            beforeLength: Int,
            afterLength: Int,
            flags: Int,
        ): SurroundingText = mirror.surrounding(beforeLength, afterLength)

        override fun commitCorrection(correctionInfo: CorrectionInfo?): Boolean {
            Log.d(TAG, "commitCorrection $correctionInfo")
            return true
        }

        override fun commitCompletion(text: CompletionInfo?): Boolean {
            Log.d(TAG, "commitCompletion $text")
            return true
        }

        override fun setSelection(start: Int, end: Int): Boolean {
            Log.d(TAG, "setSelection $start..$end (not forwarded)")
            return true
        }

        override fun finishComposingText(): Boolean {
            NativeBridge.nativeImeFinishComposingText(0)
            return true
        }

        override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
            NativeBridge.nativeImeDeleteSurroundingText(0, beforeLength, afterLength)
            return true
        }

        override fun sendKeyEvent(event: KeyEvent): Boolean {
            NativeBridge.nativeImeSendKeyEvent(
                0, event.action, event.keyCode, event.metaState, event.repeatCount
            )
            return true
        }

        // A keyboard-inserted image becomes a composer attachment, like a picked file.
        override fun commitContent(info: InputContentInfo, flags: Int, opts: Bundle?): Boolean {
            val grant = flags and InputConnection.INPUT_CONTENT_GRANT_READ_URI_PERMISSION != 0
            if (grant) {
                runCatching { info.requestPermission() }.onFailure {
                    Log.w(TAG, "commitContent permission failed: $it")
                    return false
                }
            }
            PickedFiles.deliver(view.context, listOf(info.contentUri)) {
                if (grant) info.releasePermission()
            }
            return true
        }

        override fun performEditorAction(actionCode: Int): Boolean {
            NativeBridge.nativeImePerformEditorAction(0, actionCode)
            return true
        }

        override fun getTextBeforeCursor(n: Int, flags: Int): CharSequence = mirror.before(n)
        override fun getTextAfterCursor(n: Int, flags: Int): CharSequence = mirror.after(n)
        override fun getSelectedText(flags: Int): CharSequence? = mirror.selected()

        override fun getCursorCapsMode(reqModes: Int): Int {
            val before = mirror.before(64).toString()
            return TextUtils.getCapsMode(before, before.length, reqModes)
        }
    }
}
