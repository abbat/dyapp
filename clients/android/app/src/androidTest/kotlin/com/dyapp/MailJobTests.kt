package com.dyapp

import android.app.job.JobScheduler
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class MailJobTests {
    @Test
    fun theAppSchedulesAPersistedPeriodicReceiveThatNeedsANetwork() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val job = checkNotNull(context.getSystemService(JobScheduler::class.java).getPendingJob(MAIL_JOB_ID))
        assertTrue(job.isPeriodic)
        assertTrue(job.isPersisted)
        assertNotNull(job.requiredNetwork)
    }
}
