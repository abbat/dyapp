package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AppTests {
    @Test
    fun builtApplicationHasExpectedIdentity() {
        assertEquals("com.dyapp", BuildConfig.APPLICATION_ID)
        assertEquals("0.0.1", BuildConfig.VERSION_NAME)
        assertTrue(BuildConfig.VERSION_CODE > 0)
    }
}
