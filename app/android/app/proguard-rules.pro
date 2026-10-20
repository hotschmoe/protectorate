# shared_preferences_android and jni call these Kotlin types and members from
# Dart by name and signature, which R8 cannot see.
-keep class io.flutter.plugins.sharedpreferences.** { *; }
-keep interface kotlin.coroutines.Continuation {
    public void resumeWith(java.lang.Object);
}
-keep class kotlin.jvm.internal.DefaultConstructorMarker
-keep class kotlin.Result {
    java.lang.Object value;
}
-keep class kotlin.Result$Failure {
    public <init>(java.lang.Throwable);
    java.lang.Throwable exception;
}
-keep class kotlin.Unit {
    public static kotlin.Unit INSTANCE;
}
-keep enum kotlin.coroutines.intrinsics.CoroutineSingletons
-keep class kotlin.coroutines.intrinsics.IntrinsicsKt
-keep class kotlin.coroutines.intrinsics.IntrinsicsKt__IntrinsicsKt {
    public static java.lang.Object getCOROUTINE_SUSPENDED();
}
