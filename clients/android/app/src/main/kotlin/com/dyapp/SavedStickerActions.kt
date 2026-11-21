package com.dyapp

import uniffi.dyapp.SavedStickerPackPage

/** View callbacks retain their displayed page/row; the controller rechecks admission before IO. */
internal class SavedStickerActions(
    val pick: () -> Unit,
    val navigate: (ByteArray?, SavedStickerPackPage?) -> Unit,
    val confirm: (SavedStickerRemoval) -> Unit,
    val close: () -> Unit,
)
