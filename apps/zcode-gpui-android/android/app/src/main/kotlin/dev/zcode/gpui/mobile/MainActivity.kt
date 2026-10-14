package dev.zcode.gpui.mobile

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.util.Log
import android.view.ViewGroup
import android.view.inputmethod.InputMethodManager
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import com.google.androidgamesdk.GameActivity

/**
 * GameActivity hosting the gpui surface. Methods below are invoked from
 * Rust (gpui_android) over JNI on the native thread, so every view
 * operation hops to the UI thread.
 */
class MainActivity : GameActivity() {
    private val mirror = ImeMirror()
    private var host: ImeHostView? = null
    @Volatile private var imeShown = false

    private val imm: InputMethodManager
        get() = getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // GameActivity draws edge-to-edge; opt back out so the system sizes the
        // surface between the status bar and the navigation bar or keyboard
        // (adjustResize). Targeting SDK 35+ removes this opt-out.
        WindowCompat.setDecorFitsSystemWindows(window, true)
        val view = ImeHostView(this, mirror)
        addContentView(view, ViewGroup.LayoutParams(1, 1))
        host = view
        ViewCompat.setOnApplyWindowInsetsListener(view) { v, insets ->
            val visible = insets.isVisible(WindowInsetsCompat.Type.ime())
            if (visible != imeShown) {
                imeShown = visible
                NativeBridge.nativeSetSoftKeyboardVisible(visible)
                Log.i(TAG, "IME visible=$visible")
            }
            ViewCompat.onApplyWindowInsets(v, insets)
        }
    }

    // ---- IME (called from Rust) -------------------------------------

    fun showIme() = runOnUiThread {
        val view = host ?: return@runOnUiThread
        view.requestFocus()
        imm.showSoftInput(view, 0)
    }

    fun hideIme() = runOnUiThread {
        val view = host ?: return@runOnUiThread
        imm.hideSoftInputFromWindow(view.windowToken, 0)
    }

    fun toggleIme() = if (imeShown) hideIme() else showIme()

    fun restartImeForTarget(modeId: Int) = runOnUiThread {
        val view = host ?: return@runOnUiThread
        if (view.mode == modeId) return@runOnUiThread
        view.mode = modeId
        imm.restartInput(view)
    }

    fun updateImeTextState(
        text: String,
        windowStart: Int,
        selStart: Int,
        selEnd: Int,
        compStart: Int,
        compEnd: Int,
    ) {
        mirror.text = text
        mirror.windowStart = windowStart
        mirror.selStart = selStart
        mirror.selEnd = selEnd
        mirror.compStart = compStart
        mirror.compEnd = compEnd
        runOnUiThread {
            val view = host ?: return@runOnUiThread
            imm.updateSelection(view, selStart, selEnd, compStart, compEnd)
        }
    }

    // ---- Other gpui_android hooks (no-ops on a phone) ----------------

    fun setCapturedCursorStyle(style: Int) {}
    fun setTrackpadCursorPosition(x: Float, y: Float) {}
    fun setTrackpadModeActive(active: Boolean) {}
    fun setProgrammingExtrasRowEnabled(enabled: Boolean) {}
    fun setSoftKeyboardEnabled(enabled: Boolean) {}
    fun appVersionName(): String = "0.1.0"

    fun openUrl(url: String) = runOnUiThread {
        runCatching { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
            .onFailure { Log.w(TAG, "openUrl failed: $it") }
    }

    private companion object {
        const val TAG = "zcode_gpui"
    }
}
