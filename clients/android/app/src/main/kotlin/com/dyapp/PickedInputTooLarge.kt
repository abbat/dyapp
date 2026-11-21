package com.dyapp

import java.io.IOException

internal class PickedInputTooLarge : IOException("Picked input exceeds the native byte limit")
