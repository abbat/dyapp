package com.dyapp

import android.app.Application
import android.content.Context

/** Process start: gives the Rust DNS resolver this app's context before any network work. */
class DYApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        // JNA loads the same library for UniFFI; JNI needs it registered here as well.
        System.loadLibrary("dyapp_ffi")
        nativeInit(this)
    }

    private external fun nativeInit(context: Context)
}
