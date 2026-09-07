package dev.clawseed.demo.scheduled

import java.util.Calendar
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class AlarmScheduleTest {
    private fun time(day: Int, hour: Int = 8, minute: Int = 0): Long = Calendar.getInstance().apply {
        clear()
        set(2026, Calendar.SEPTEMBER, day, hour, minute)
    }.timeInMillis

    private fun alarm(days: List<Int>) = ScheduledTask(
        name = "Alarm", message = "Wake up", hour = 8, minute = 0,
        repeat = AlarmSchedule.repeat(days), repeatDays = days, isAlarm = true,
    )

    @Test fun saturdayAlarmSkipsOtherDaysAndAdvancesAWeekAfterFiring() {
        val task = alarm(listOf(6))
        assertEquals(TaskRepeat.CUSTOM, task.repeat)
        assertEquals(time(12), ScheduledTaskManager.nextTriggerMillis(task, time(7)))
        assertEquals(time(19), ScheduledTaskManager.nextTriggerMillis(task, time(12)))
    }

    @Test fun selectedDaysKeepTheirExactSchedule() {
        val task = alarm(listOf(1, 3))
        assertEquals(time(7), ScheduledTaskManager.nextTriggerMillis(task, time(7, 7)))
        assertEquals(time(9), ScheduledTaskManager.nextTriggerMillis(task, time(7, 9)))
    }

    @Test fun existingRepeatModesKeepTheirBehavior() {
        assertEquals(time(8), ScheduledTaskManager.nextTriggerMillis(alarm(emptyList()), time(7)))
        assertEquals(time(13), ScheduledTaskManager.nextTriggerMillis(alarm((1..7).toList()), time(12)))
        assertEquals(time(14), ScheduledTaskManager.nextTriggerMillis(alarm((1..5).toList()), time(12)))
    }

    @Test fun daysAreValidatedNormalizedAndAcceptNull() {
        assertEquals(emptyList<Int>(), AlarmSchedule.parseDays(Json.parseToJsonElement("null")))
        assertEquals(listOf(1, 6), AlarmSchedule.parseDays(Json.parseToJsonElement("[6,1,6]")))
        for (invalid in listOf("[0]", "[8]", "[1,null]", "[1.5]", "{}")) {
            assertTrue(runCatching { AlarmSchedule.parseDays(Json.parseToJsonElement(invalid)) }.isFailure)
        }
    }

    @Test fun selectedDaysSurvivePersistenceAndOldTasksStillDecode() {
        val task = alarm(listOf(6, 7))
        assertEquals(task, Json.decodeFromString<ScheduledTask>(Json.encodeToString(task)))
        val old = Json.decodeFromString<ScheduledTask>("""{"name":"old","message":"hello","hour":8,"minute":0,"repeat":"WEEKDAY"}""")
        assertEquals(emptyList<Int>(), old.repeatDays)
        assertEquals(TaskRepeat.WEEKDAY, old.repeat)
    }
}
