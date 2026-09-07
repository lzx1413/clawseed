package dev.clawseed.demo.ui.scheduled

import dev.clawseed.demo.scheduled.ScheduledTask
import dev.clawseed.demo.scheduled.ScheduledTaskManager
import dev.clawseed.demo.scheduled.TaskRepeat
import dev.clawseed.demo.scheduled.TaskStatus
import org.junit.Assert.*
import org.junit.Test

class TaskPresentationTest {
    private val task = ScheduledTask(name = "Test", message = "Test", hour = 8, minute = 0)
    private val now = 1_788_775_200_000L

    @Test fun displayUsesTheSameScheduleAsTheAlarm() {
        for (repeat in TaskRepeat.entries) {
            val candidate = task.copy(repeat = repeat, repeatDays = listOf(2, 4))
            assertEquals(ScheduledTaskManager.nextTriggerMillis(candidate, now), nextScheduledRun(candidate, now))
        }
    }

    @Test fun disabledAndMalformedTasksDoNotDisplayAnUpcomingRun() {
        assertNull(nextScheduledRun(task.copy(enabled = false), now))
        assertNull(nextScheduledRun(task.copy(hour = 24), now))
        assertNull(nextScheduledRun(task.copy(repeat = TaskRepeat.CUSTOM), now))
        assertNull(nextScheduledRun(task.copy(repeat = TaskRepeat.CUSTOM, repeatDays = listOf(9)), now))
    }

    @Test fun previousFailureDoesNotMeanTheScheduleIsDisabled() {
        assertNotNull(nextScheduledRun(task.copy(lastStatus = TaskStatus.FAILED), now))
    }
}
