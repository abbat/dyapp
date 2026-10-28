# 0032. Android context for the DNS resolver

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

Every client swarm adds the libp2p DNS transport (`with_dns`), which takes its name servers from
`hickory-resolver`'s system configuration. On Android that code asks `ConnectivityManager` for
the active network's DNS servers through JNI, using the Java VM and `Context` stored in
`ndk-context`. The app is a Kotlin activity that loads `libdyapp_ffi.so` through JNA, so nothing
set that context: building a swarm panicked, and every network check and publication on Android
failed before the first dial.

Other options were passing the DNS servers from Kotlin through every check and publish call
(more plumbing, and the list would not follow a network change), dropping DNS seeds on Android
(seed host names would stop working there) and a fixed public resolver (seed names would go to a
third party and the user's DNS and VPN would be ignored).

## Decision

On Android, `dyapp-ffi` depends on `ndk-context` and `jni-sys`, both already built for Android
as dependencies of `hickory-resolver`. A JNI export, `Java_com_dyapp_DYApplication_nativeInit`,
stores the Java VM and a global reference to the application `Context` once per process; the
app's `Application` subclass calls it in `onCreate`, before any activity or service starts.

## Consequences

Android resolves seed names through the system DNS servers, as desktop does. `dyapp-ffi` has a
small hand-written unsafe JNI function, built only for Android.
