package dev.clawseed.demo.ui.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.width
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.clawseed.demo.ui.theme.ClawSeedTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class UnifiedComponentsTest {
    @get:Rule val compose = createComposeRule()

    @Test fun switchRowTogglesExactlyOnce() {
        var calls = 0
        compose.setContent {
            var checked by remember { mutableStateOf(false) }
            ClawSeedTheme { SettingsSwitchRow("Switch", checked, { checked = it; calls++ }) }
        }
        compose.onNodeWithText("Switch").performClick().assertIsOn()
        compose.runOnIdle { assertEquals(1, calls) }
        compose.onAllNodes(SemanticsMatcher.expectValue(SemanticsProperties.Role, Role.Switch)).assertCountEquals(1)
    }

    @Test fun loadingButtonBlocksRepeatSubmissionAndKeepsWidth() {
        var calls = 0
        compose.setContent {
            var loading by remember { mutableStateOf(false) }
            ClawSeedTheme { ClawSeedButton({ calls++; loading = true }, Modifier.testTag("save"), loading = loading) { Text("Save") } }
        }
        val width = compose.onNodeWithTag("save").fetchSemanticsNode().boundsInRoot.width
        compose.onNodeWithTag("save").performClick().assertIsNotEnabled()
        compose.onNodeWithTag("save").performClick()
        compose.runOnIdle { assertEquals(1, calls) }
        assertEquals(width, compose.onNodeWithTag("save").fetchSemanticsNode().boundsInRoot.width)
    }

    @Test fun singleAndMultipleChoicesExposeDifferentSemantics() {
        compose.setContent {
            var selected by remember { mutableIntStateOf(0) }
            var checked by remember { mutableStateOf(false) }
            ClawSeedTheme { Column {
                ClawSeedSegmentedPill(listOf("One", "Two"), selected, { selected = it })
                ClawSeedChoiceChip(checked, { checked = !checked }, { Text("Many") }, choiceType = ChoiceType.Multiple)
            } }
        }
        compose.onNodeWithText("Two").performClick().assertIsSelected()
        compose.onNodeWithText("One").assertIsNotSelected()
        compose.onNodeWithText("Many").performClick().assertIsOn()
        compose.onNodeWithText("Many").performClick().assertIsOff()
    }

    @Test fun inputPreservesErrorAndReadOnlyBehavior() {
        var changes = 0
        compose.setContent { ClawSeedTheme {
            ClawSeedTextField("Value", { changes++ }, Modifier.testTag("field"), readOnly = true,
                isError = true, supportingText = { Text("Invalid value") })
        } }
        compose.onNodeWithText("Invalid value").assertIsDisplayed()
        compose.onNodeWithTag("field").assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Error))
        compose.runOnIdle { assertEquals(0, changes) }
    }
    @Test fun cardSecondaryActionDoesNotOpenTheCard() {
        var opened = 0
        var edited = 0
        compose.setContent { ClawSeedTheme {
            ClawSeedCard(onClick = { opened++ }) {
                Text("Card title")
                ClawSeedIconButton({ edited++ }, contentDescription = "Edit card") { Text("Edit") }
            }
        } }
        compose.onNodeWithContentDescription("Edit card").performClick()
        compose.runOnIdle { assertEquals(0, opened); assertEquals(1, edited) }
        compose.onNodeWithText("Card title").performClick()
        compose.runOnIdle { assertEquals(1, opened) }
    }

    @Test fun drawerSupportsLargeTextAndIndependentSecondaryActions() {
        var selected = 0
        var actions = 0
        compose.setContent { ClawSeedTheme {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, 2f)) {
                ClawSeedNavigationRow(
                    label = { Text("A long session title that must wrap", Modifier.testTag("title")) },
                    selected = true, onClick = { selected++ }, modifier = Modifier.width(280.dp),
                    badge = { ClawSeedIconButton({ actions++ }, contentDescription = "Session actions") { Text("...") } },
                )
            }
        } }
        val title = compose.onNodeWithTag("title", useUnmergedTree = true).fetchSemanticsNode().boundsInRoot
        val action = compose.onNodeWithContentDescription("Session actions").fetchSemanticsNode().boundsInRoot
        org.junit.Assert.assertTrue("Title must not overlap actions", title.right <= action.left)
        compose.onNodeWithContentDescription("Session actions").performClick()
        compose.runOnIdle { assertEquals(0, selected); assertEquals(1, actions) }
        compose.onNodeWithText("A long session title that must wrap").performClick().assertIsSelected()
        compose.runOnIdle { assertEquals(1, selected) }
    }

    @Test fun narrowLargeTextSegmentsRemainSelectable() {
        compose.setContent { ClawSeedTheme {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, 2f)) {
                var selected by remember { mutableIntStateOf(0) }
                ClawSeedSegmentedPill(listOf("Light theme", "Dark theme", "System theme"), selected,
                    { selected = it }, Modifier.width(280.dp))
            }
        } }
        compose.onNodeWithText("System theme").performClick().assertIsSelected()
        compose.onNodeWithText("Light theme").assertIsNotSelected()
        val first = compose.onNodeWithText("Light theme").fetchSemanticsNode().boundsInRoot
        val last = compose.onNodeWithText("System theme").fetchSemanticsNode().boundsInRoot
        org.junit.Assert.assertTrue("Large labels should use vertical segments", first.bottom <= last.top)
    }

}
