package com.dyapp

/** Existing receiving matrix delegates unchanged metadata and checks to the shared screen writer. */
internal class ReceiveVisualOutput(source: String, width: Int) {
    private val output = NativeScreenVisualOutput(
        source,
        width,
        "receive-visual",
        ReceiveVisualState.entries.size * 2 * 2 * 2 * 2
    )

    fun capture(host: ReceiveVisualHost, scene: ReceiveVisualScene, edge: String) = output.capture(
        host.compose,
        NativeScreenVisualScene(
            "settings-receive",
            scene.state.screen,
            scene.context,
            scene.dark,
            scene.fontScale
        ),
        edge
    )

    fun finish() = output.finish()
}
