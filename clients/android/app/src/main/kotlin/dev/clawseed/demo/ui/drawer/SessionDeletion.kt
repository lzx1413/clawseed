package dev.clawseed.demo.ui.drawer

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

internal data class SessionDeletionResult(val deleted: Set<String>, val failed: Set<String>)

/** Delete only the confirmed snapshot; retain failed sessions and report each success immediately. */
internal suspend fun deleteHistorySessions(
    sessionIds: List<String>,
    delete: suspend (String) -> Result<Unit>,
    onDeleted: (String) -> Unit,
): SessionDeletionResult {
    val deleted = mutableSetOf<String>()
    val failed = mutableSetOf<String>()
    for (id in sessionIds.distinct()) {
        currentCoroutineContext().ensureActive()
        val result = try {
            delete(id)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Result.failure(e)
        }
        // Some SDK calls wrap cancellation in Result; never continue a cancelled batch.
        (result.exceptionOrNull() as? CancellationException)?.let { throw it }
        if (result.isSuccess) {
            deleted += id
            onDeleted(id)
        } else {
            failed += id
        }
    }
    return SessionDeletionResult(deleted, failed)
}
