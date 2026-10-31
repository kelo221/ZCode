package dev.zcode.gpui.mobile

// Package and name are fixed by the Rust JNI symbols in gpui_android
// (`Java_dev_zcode_gpui_mobile_NativeBridge_*`). Window id 0 = the main activity.
object NativeBridge {
    @JvmStatic external fun nativeImeCommitText(windowId: Long, text: String, newCursorPosition: Int)
    @JvmStatic external fun nativeImeSetComposingText(windowId: Long, text: String, newCursorPosition: Int)
    @JvmStatic external fun nativeImeFinishComposingText(windowId: Long)
    @JvmStatic external fun nativeImeSetComposingRegion(windowId: Long, start: Int, end: Int)
    @JvmStatic external fun nativeImeReplaceText(windowId: Long, start: Int, end: Int, text: String)
    @JvmStatic external fun nativeImeDeleteSurroundingText(windowId: Long, beforeLength: Int, afterLength: Int)
    @JvmStatic external fun nativeImeSendKeyEvent(windowId: Long, action: Int, keyCode: Int, metaState: Int, repeatCount: Int)
    @JvmStatic external fun nativeImePerformEditorAction(windowId: Long, actionId: Int)
    @JvmStatic external fun nativeImeRouteAsKeys(): Boolean
    @JvmStatic external fun nativeSetSoftKeyboardVisible(visible: Boolean)
    // Defined in apps/zcode-gpui-android/src/lib.rs, not gpui_android.
    @JvmStatic external fun nativeSubmitPairLink(link: String)
    @JvmStatic external fun nativeAttachFile(name: String, mime: String, bytes: ByteArray)
    @JvmStatic external fun nativeAttachError(message: String)
}
