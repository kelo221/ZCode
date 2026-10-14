package dev.zcode.gpui.mobile

import android.content.Context
import android.text.InputType
import android.text.TextUtils
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection

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
        outAttrs.initialSelStart = mirror.selStart
        outAttrs.initialSelEnd = mirror.selEnd
        return Connection(this, mirror)
    }

    private class Connection(view: View, private val mirror: ImeMirror) :
        BaseInputConnection(view, false) {

        override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean {
            NativeBridge.nativeImeCommitText(0, text?.toString() ?: "", newCursorPosition)
            return true
        }

        override fun setComposingText(text: CharSequence?, newCursorPosition: Int): Boolean {
            NativeBridge.nativeImeSetComposingText(0, text?.toString() ?: "", newCursorPosition)
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
