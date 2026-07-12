package zip.iptables.pandar.android.ui.printerdetail

import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import zip.iptables.pandar.android.data.remote.dto.PrinterAxis

@RunWith(AndroidJUnit4::class)
class AxisControlsTest {
    @get:Rule val composeRule = createComposeRule()

    @Test fun every_signed_button_dispatches_its_exact_axis_and_distance() {
        val calls = mutableListOf<Pair<PrinterAxis, Double>>()
        composeRule.setContent {
            MaterialTheme {
                AxisControls(
                    enabled = true,
                    onHome = {},
                    onMoveAxis = { axis, deltaMm -> calls += axis to deltaMm },
                )
            }
        }
        val cases = PrinterAxis.entries.flatMap { axis ->
            listOf(-10.0, -1.0, 1.0, 10.0).map { deltaMm -> axis to deltaMm }
        }
        cases.forEach { (axis, deltaMm) ->
            val signed = if (deltaMm > 0) "+${deltaMm.toInt()}" else deltaMm.toInt().toString()
            composeRule
                .onNodeWithContentDescription("Move ${axis.name} by $signed mm")
                .performClick()
        }
        composeRule.runOnIdle { assertEquals(cases, calls) }
    }

    @Test fun home_requires_confirmation() {
        var homeCalls = 0
        composeRule.setContent {
            MaterialTheme {
                AxisControls(
                    enabled = true,
                    onHome = { homeCalls += 1 },
                    onMoveAxis = { _, _ -> },
                )
            }
        }
        composeRule.onNodeWithText("Home all axes").performClick()
        composeRule.runOnIdle { assertEquals(0, homeCalls) }
        composeRule.onNodeWithText("Are you sure you want to trigger auto homing?").assertIsDisplayed()
        composeRule.onNodeWithText("Homing").performClick()
        composeRule.runOnIdle { assertEquals(1, homeCalls) }
    }

    @Test fun in_flight_disables_all_printer_command_buttons() {
        composeRule.setContent {
            MaterialTheme {
                AxisControls(enabled = false, onHome = {}, onMoveAxis = { _, _ -> })
            }
        }
        PrinterAxis.entries.forEach { axis ->
            listOf(-10, -1, 1, 10).forEach { deltaMm ->
                val signed = if (deltaMm > 0) "+$deltaMm" else deltaMm.toString()
                composeRule
                    .onNodeWithContentDescription("Move ${axis.name} by $signed mm")
                    .assertIsNotEnabled()
            }
        }
        composeRule.onNodeWithText("Home all axes").assertIsNotEnabled()
    }
}
