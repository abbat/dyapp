package com.dyapp

internal class IdentitySettingsActions(val act: (() -> Opened) -> Unit, val lock: () -> Unit)
