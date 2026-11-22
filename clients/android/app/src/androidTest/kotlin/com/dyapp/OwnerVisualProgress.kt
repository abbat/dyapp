package com.dyapp

import android.os.Bundle
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.rules.TestName

/** Only a completed case, including cleanup, renews the dedicated runner's failure deadline. */
internal class OwnerVisualProgress(private val suite: String, private val method: TestName) {
    private var completed = 0

    fun completed() {
        completed += 1
        val status = Bundle().apply {
            putString("id", "DYAppVisual")
            putString("class", suite)
            putString("test", method.methodName)
            putString("case", completed.toString())
        }
        InstrumentationRegistry.getInstrumentation().sendStatus(2, status)
    }
}
