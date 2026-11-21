package com.dyapp

import uniffi.dyapp.SavedStickerPackEntry

/** Each confirmation opening is distinct even when it shows the same unchanged entry. */
internal class SavedStickerRemoval(val entry: SavedStickerPackEntry)
