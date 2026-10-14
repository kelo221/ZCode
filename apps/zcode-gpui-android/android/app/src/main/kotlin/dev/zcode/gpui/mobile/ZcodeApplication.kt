package dev.zcode.gpui.mobile

import android.app.Application

class ZcodeApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        // GameActivity dlopen()s the library itself, but JNI only binds
        // `external fun` natives for libraries loaded via System.loadLibrary.
        System.loadLibrary("zcode_gpui_android")
    }
}
