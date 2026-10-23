package dev.protectorate.protectorate

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.spec.ECGenParameterSpec

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
        // One non-exportable P-256 key per hub profile in the Android
        // Keystore. Dart sees only the public key and signatures.
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "protectorate/device_key")
            .setMethodCallHandler { call, result ->
                try {
                    val keystore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
                    when (call.method) {
                        "create" -> {
                            val alias = call.arguments as String
                            val generator = KeyPairGenerator.getInstance(
                                KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore")
                            generator.initialize(
                                KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_SIGN)
                                    .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                                    .setDigests(KeyProperties.DIGEST_SHA256)
                                    .build())
                            result.success(generator.generateKeyPair().public.encoded)
                        }
                        "publicKey" -> result.success(
                            keystore.getCertificate(call.arguments as String)?.publicKey?.encoded)
                        "sign" -> {
                            val key = keystore.getKey(call.argument<String>("alias"), null)
                                as PrivateKey
                            val signature = Signature.getInstance("SHA256withECDSA")
                            signature.initSign(key)
                            signature.update(call.argument<ByteArray>("message"))
                            result.success(signature.sign())
                        }
                        "delete" -> {
                            keystore.deleteEntry(call.arguments as String)
                            result.success(null)
                        }
                        else -> result.notImplemented()
                    }
                } catch (error: Exception) {
                    result.error("device_key", error.message, null)
                }
            }
    }
}
