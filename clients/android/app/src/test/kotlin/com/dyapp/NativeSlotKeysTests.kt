package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Test

class NativeSlotKeysTests {
    @Test
    fun commonCallbackKeepsTheExistingLegacyAndUuidAliases() {
        val id = "11111111-1111-4111-8111-111111111111"
        assertEquals("dyapp-identity-wrap", identityAlias("legacy-primary"))
        assertEquals("dyapp-identity-wrap:$id", identityAlias(id))
    }
}
