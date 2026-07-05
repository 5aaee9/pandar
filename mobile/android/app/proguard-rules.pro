# Pandar Android has no special ProGuard/R8 rules yet (minify disabled in debug/release).
# If minification is enabled later, keep kotlinx.serialization and Retrofit model classes:
# -keep class zip.iptables.pandar.android.** { *; }
# -keepclassmembers class kotlin.Metadata { *; }
