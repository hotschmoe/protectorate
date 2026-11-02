package com.hotschmoe.protectorate

import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "protectorate/files")
            .setMethodCallHandler { call, result ->
                if (call.method == "filesDir") result.success(filesDir.absolutePath)
                else result.notImplemented()
            }
    }
}
