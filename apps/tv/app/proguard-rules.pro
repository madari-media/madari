-keep class dev.madari.tv.NativeCore { *; }
# Referenced by Rust JNI by class/method name; R8 cannot see those calls.
-keep class org.rustls.platformverifier.** { *; }
