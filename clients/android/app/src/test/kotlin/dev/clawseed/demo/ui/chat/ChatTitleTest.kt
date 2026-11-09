package dev.clawseed.demo.ui.chat

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ChatTitleTest {
    @Test
    fun trimsTitleBeforeSaving() {
        assertEquals("项目进度", normalizeSessionTitle("  项目进度  "))
    }

    @Test
    fun rejectsBlankTitle() {
        assertNull(normalizeSessionTitle(" \n\t "))
    }
}
