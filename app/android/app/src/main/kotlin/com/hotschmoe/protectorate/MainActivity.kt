package com.hotschmoe.protectorate

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.util.Log
import android.widget.Toast
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import org.json.JSONObject
import java.io.File

class MainActivity : FlutterActivity() {
    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "protectorate/files")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "filesDir" -> result.success(filesDir.absolutePath)
                    "cacheDir" -> result.success(cacheDir.absolutePath)
                    "install" -> install(call.argument<String>("path") ?: "", call.argument<String>("version") ?: "", result)
                    "lastInstall" -> result.success(InstallRecord.read(this)?.toString())
                    "clearInstall" -> {
                        InstallRecord.clear(this)
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
    }

    /** "allow" when the owner must first let this app install apps, else "committed". */
    private fun install(path: String, version: String, result: MethodChannel.Result) {
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
                    InstallRecord.write(
                        this,
                        JSONObject().put("version", version).put("attempt_at", System.currentTimeMillis())
                    )
                    val mutable = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0
                    val status = PendingIntent.getBroadcast(
                        this, id, Intent(this, InstallStatus::class.java),
                        PendingIntent.FLAG_UPDATE_CURRENT or mutable
                    )
                    session.commit(status.intentSender)
                }
                runOnUiThread { result.success("committed") }
            } catch (e: Exception) {
                InstallRecord.clear(this)
                runOnUiThread { result.error("install", e.toString(), null) }
            }
        }.start()
    }
}

/**
 * The last install attempt in filesDir, read by the app on its next start: Android kills the app
 * while it replaces it, so neither the toast nor the Dart side is there to see the answer.
 */
object InstallRecord {
    private const val NAME = "install-status.json"
    private const val MAX_TEXT = 500

    fun bound(text: String?): String = (text ?: "").take(MAX_TEXT)

    @Synchronized
    fun read(context: Context): JSONObject? = try {
        val file = File(context.filesDir, NAME)
        if (file.exists()) JSONObject(file.readText()) else null
    } catch (e: Exception) {
        Log.w("InstallRecord", "read failed", e)
        null
    }

    @Synchronized
    fun write(context: Context, record: JSONObject) {
        try {
            val tmp = File(context.filesDir, "$NAME.tmp")
            tmp.writeText(record.toString())
            if (!tmp.renameTo(File(context.filesDir, NAME))) Log.w("InstallRecord", "rename failed")
        } catch (e: Exception) {
            Log.w("InstallRecord", "write failed", e)
        }
    }

    @Synchronized
    fun clear(context: Context) {
        File(context.filesDir, NAME).delete()
    }
}

/** The installer session's answer: the system's confirm dialog, or why it failed. */
class InstallStatus : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        val message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)
        val record = (InstallRecord.read(context) ?: JSONObject())
            .put("status", status)
            .put("status_at", System.currentTimeMillis())
            .put("message", InstallRecord.bound(message))
            .put("package", InstallRecord.bound(intent.getStringExtra(PackageInstaller.EXTRA_PACKAGE_NAME)))
        InstallRecord.write(context, record)
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                val confirm = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)
                }
                if (confirm == null) {
                    fail(context, record, "Android sent no confirm dialog")
                    return
                }
                try {
                    context.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                } catch (e: Exception) {
                    fail(context, record, "the confirm dialog did not open: $e")
                }
            }
            PackageInstaller.STATUS_SUCCESS -> {}
            else -> toast(context, message ?: "status $status")
        }
    }

    private fun fail(context: Context, record: JSONObject, why: String) {
        InstallRecord.write(
            context,
            record.put("status", PackageInstaller.STATUS_FAILURE).put("message", InstallRecord.bound(why))
        )
        toast(context, why)
    }

    private fun toast(context: Context, why: String) =
        Toast.makeText(context, "Update failed: $why", Toast.LENGTH_LONG).show()
}
