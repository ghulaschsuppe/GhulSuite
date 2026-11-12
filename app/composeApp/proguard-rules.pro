# SPDX-License-Identifier: AGPL-3.0-or-later
# JNA and the UniFFI bindings use reflection and native callbacks: keep them unchanged.
-keep class com.sun.jna.** { *; }
-keep class * extends com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class de.danerd.ghulsuite.core.** { *; }
-dontwarn java.awt.**
-dontwarn com.sun.jna.**

# Callbacks implemented in Kotlin and called from Rust.
-keep class * implements de.danerd.ghulsuite.core.HidTransport { *; }
-keep class * implements de.danerd.ghulsuite.core.FidoListener { *; }
