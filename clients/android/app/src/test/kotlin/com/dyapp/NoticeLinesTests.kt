package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.dyapp.ChatMailNotice

class NoticeLinesTests {
    private val peer = "12D3KooWpeer000001"

    @Test
    fun theCoreFixedResultAddsNothing() {
        assertNull(noticeLines(ChatMailNotice(null, null)))
        assertNull(noticeLines(ChatMailNotice(null, "must stay fixed")))
    }

    @Test
    fun theAdmittedSenderAndBodyUseExistingPresentation() {
        assertEquals(NoticeLines(shortPeerId(peer), null), noticeLines(ChatMailNotice(peer, null)))
        assertEquals(NoticeLines(shortPeerId(peer), "hi"), noticeLines(ChatMailNotice(peer, "hi")))
    }

    @Test
    fun blankCoreTextRetainsTheExistingFixedBodyFallback() {
        assertEquals(NoticeLines(shortPeerId(peer), null), noticeLines(ChatMailNotice(peer, "   ")))
    }

    @Test
    fun nativePlainTextIsNotReinterpretedAsMarkup() {
        val text = "<b>literal</b> & text"
        assertEquals(NoticeLines(shortPeerId(peer), text), noticeLines(ChatMailNotice(peer, text)))
    }
}
