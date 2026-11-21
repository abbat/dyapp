package com.dyapp

import uniffi.dyapp.MobileDeviceDirectory

/** Read only the initialized current public device; never regenerate or contact a node. */
internal fun workerDeviceKey(opened: Opened.Ready): ByteArray = MobileDeviceDirectory(
    opened.slot.owner(),
    opened.slot.id,
    opened.peerId,
    PlatformKeyStore.existing(identityAlias(opened.slot.id))
).use { directory -> directory.devices(false).single { it.current }.key }
