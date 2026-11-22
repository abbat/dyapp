package com.dyapp

/** Existing shell routes and identity actions travel together through presentation components. */
internal class ShellControls(
    val lockMode: LockMode,
    val act: (() -> Opened) -> Unit,
    val open: (StartTarget?) -> Unit
)
