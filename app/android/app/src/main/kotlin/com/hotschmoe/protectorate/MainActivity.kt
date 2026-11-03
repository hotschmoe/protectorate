package com.hotschmoe.protectorate

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.widget.Toast
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "protectorate/files")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "filesDir" -> result.success(filesDir.absolutePath)
                    "cacheDir" -> result.success(cacheDir.absolutePath)
                    "install" -> install(call.argument<String>("path") ?: "", result)
                    else -> result.notImplemented()
                }
            }
    }

    /** "allow" when the owner must first let this app install apps, else "committed". */
    private fun install(path: String, result: MethodChannel.Result) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !packageManager.canRequestPackageInstalls()) {
            startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:$packageName")))
            result.success("allow")
            return
        }
        Thread {
            try {
                val apk = File(path)
                val installer = packageManager.packageInstaller
                val id = installer.createSession(
                    PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL)
                )
                installer.openSession(id).use { session ->
                    apk.inputStream().use { input ->
                        session.openWrite("protectorate.apk", 0, apk.length()).use { out ->
                            input.copyTo(out)
                            session.fsync(out)
                        }
                    }
                    val mutable = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0
                    val status = PendingIntent.getBroadcast(
                        this, id, Intent(this, InstallStatus::class.java),
                        PendingIntent.FLAG_UPDATE_CURRENT or mutable
                    )
                    session.commit(status.intentSender)
                }
                runOnUiThread { result.success("committed") }
            } catch (e: Exception) {
                runOnUiThread { result.error("install", e.toString(), null) }
            }
        }.start()
    }
}

/** The installer session's answer: the system's confirm dialog, or why it failed. */
class InstallStatus : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                val confirm = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)
                }
                confirm?.let { context.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            }
            PackageInstaller.STATUS_SUCCESS -> {}
            else -> {
                val why = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "status $status"
                Toast.makeText(context, "Update failed: $why", Toast.LENGTH_LONG).show()
            }
        }
    }
}
