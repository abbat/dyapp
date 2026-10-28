# 0032. Android context for the DNS resolver

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

Every client swarm adds the libp2p DNS transport, which takes its name servers from
`hickory-resolver`'s system configuration. On Android that code asks `ConnectivityManager` for
the active network's DNS servers through JNI, using the Java VM and `Context` stored in
`ndk-context`. The app is a Kotlin activity that loads `libdyapp_ffi.so` through JNA, so nothing
set that context: building a swarm panicked, and every network check and publication on Android
failed before the first dial.

Other options were passing the DNS servers from Kotlin through every check and publish call
(more plumbing, and the list would not follow a network change), dropping DNS seeds on Android
(seed host names would stop working there) and a fixed public resolver (seed names would go to a
third party and the user's DNS and VPN would be ignored).

With the context set, the same lookup still fails when the device has no active network:
`getActiveNetwork` returns null and the next JNI call refuses it, so `with_dns` failed and the
swarm, and with it the network check, was never built. A host without `/etc/resolv.conf` fails
the same way.

## Decision

On Android, `dyapp-ffi` depends on `ndk-context` and `jni-sys`, both already built for Android
as dependencies of `hickory-resolver`. A JNI export, `Java_com_dyapp_DYApplication_nativeInit`,
stores the Java VM and a global reference to the application `Context` once per process; the
app's `Application` subclass calls it in `onCreate`, before any activity or service starts. The
manifest requests `ACCESS_NETWORK_STATE`, which `ConnectivityManager.getActiveNetwork` needs.

`dyapp-p2p-net` depends on `hickory-resolver` directly (the version libp2p already uses) and
reads the system configuration itself before building a swarm, passing it to
`with_dns_config`. When that read fails, the swarm gets no name servers and logs a warning: IP
addresses still dial, host names fail in that swarm, and the next swarm reads the configuration
again.

## Consequences

Android resolves seed names through the system DNS servers, as desktop does. `dyapp-ffi` has a
small hand-written unsafe JNI function, built only for Android. A swarm built while offline does
not resolve names even after the network returns: a mobile client's next check or publication
builds a new swarm and does, a long-running node only after a restart.
