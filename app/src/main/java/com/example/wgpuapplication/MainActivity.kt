package com.example.wgpuapplication

import android.os.Bundle
import android.util.Log
import com.google.androidgamesdk.GameActivity

class MainActivity : GameActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
    }

    companion object {
        init {
            Log.i("TEST", "TEST");
            System.loadLibrary("wgpu_android_lib")
        }
    }
}