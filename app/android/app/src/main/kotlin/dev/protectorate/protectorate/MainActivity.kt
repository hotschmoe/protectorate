package dev.protectorate.protectorate

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "protectorate/browser")
            .setMethodCallHandler { call, result ->
                if (call.method != "open") {
                    result.notImplemented()
                    return@setMethodCallHandler
                }
                val uri = Uri.parse(call.arguments as? String ?: "")
                if (uri.scheme != "https" && uri.scheme != "http") {
                    result.error("invalid_url", "Release notes need an HTTP URL", null)
                    return@setMethodCallHandler
                }
                try {
                    startActivity(Intent(Intent.ACTION_VIEW, uri))
                    result.success(null)
                } catch (error: ActivityNotFoundException) {
                    result.error("no_browser", "Opening release notes: ${error.message}", null)
                }
            }
    }
}
