package com.dyapp

/** Presentation lifetime only; native admission owns recipient, network, privacy and daily policy. */
internal class ProfileViewDisclosureCalls(val retire: () -> Unit, val send: () -> Unit)
