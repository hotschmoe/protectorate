# shared_preferences_android 2.5 calls its Kotlin registrars from Dart through
# JNI by name and signature, which R8 cannot see. Keep the classes, and keep the
# Kotlin runtime types that appear in those signatures from being renamed;
# otherwise the release app throws NoSuchMethodError reading its settings and
# never draws a frame.
-keep class io.flutter.plugins.sharedpreferences.** { *; }
-keep class kotlin.coroutines.Continuation
-keep class kotlin.jvm.internal.DefaultConstructorMarker
