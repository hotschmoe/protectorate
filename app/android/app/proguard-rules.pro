# shared_preferences_android 2.5 calls SharedPreferencesApiRegistrar from Dart
# through JNI, which R8 cannot see; without this the release app throws
# NoSuchMethodError reading its settings and never draws a frame.
-keep class io.flutter.plugins.sharedpreferences.** { *; }
