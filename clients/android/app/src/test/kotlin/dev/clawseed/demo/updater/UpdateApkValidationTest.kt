package dev.clawseed.demo.updater

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UpdateApkValidationTest {
    @Test
    fun acceptsOnlyTheRequestedNewerApp() {
        fun verify(packageName: String?, version: String?, code: Long) =
            matchesUpdateApk(packageName, version, code, "dev.clawseed.demo", "2.2.0", 20)

        assertTrue(verify("dev.clawseed.demo", "2.2.0", 21))
        assertFalse(verify("dev.clawseed.demo", "2.0.0", 19))
        assertFalse(verify("dev.clawseed.demo", "2.2.0", 20))
        assertFalse(verify("other.app", "2.2.0", 21))
        assertFalse(verify(null, null, 0))
    }
}
