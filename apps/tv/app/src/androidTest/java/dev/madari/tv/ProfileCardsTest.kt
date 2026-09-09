package dev.madari.tv

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.madari.tv.feature.profiles.ProfileCard
import dev.madari.tv.ui.theme.MadariTheme
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Run on a dedicated test device, not the personal TV (see docs/android-tv.md). */
@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class ProfileCardsTest {
    @get:Rule val compose = createComposeRule()

    @Test fun fiveProfilesAndAddFitWithoutScrollingAndNavigateHorizontally() {
        val first = FocusRequester()
        var opened = ""
        compose.setContent {
            MadariTheme {
                LazyRow(
                    Modifier.width(832.dp),
                    contentPadding = PaddingValues(6.dp),
                    horizontalArrangement = Arrangement.spacedBy(24.dp)
                ) {
                    items(5) { index ->
                        ProfileCard(
                            JSONObject().put("id", "$index").put("name", "Viewer $index"),
                            if (index == 0) Modifier.focusRequester(first) else Modifier,
                            onOpen = { opened = "Viewer $index" }
                        )
                    }
                    item { ProfileCard(null, onOpen = { opened = "Add profile" }) }
                }
            }
        }
        compose.runOnIdle { first.requestFocus() }
        repeat(5) { index ->
            compose.onNodeWithContentDescription("Viewer $index").assertIsDisplayed()
                .assertWidthIsEqualTo(112.dp).assertHeightIsEqualTo(112.dp)
        }
        compose.onNodeWithContentDescription("Add profile").assertIsDisplayed()
        compose.onNodeWithText("Personal profile").assertDoesNotExist()
        compose.onNodeWithContentDescription("Viewer 0").assertIsFocused()
            .performKeyInput { pressKey(Key.DirectionRight) }
        compose.onNodeWithContentDescription("Viewer 1").assertIsFocused()
            .performKeyInput { pressKey(Key.DirectionCenter) }
        compose.runOnIdle { assertEquals("Viewer 1", opened) }
    }

    @Test fun protectedKidsCardHasAccessibleLabelAndSeparateEditAction() {
        var opened = 0
        var edited = 0
        compose.setContent {
            MadariTheme {
                ProfileCard(
                    JSONObject().put("id", "kids").put("name", "A long profile name")
                        .put("kids", true).put("pin_protected", true),
                    onOpen = { opened++ },
                    onEdit = { edited++ }
                )
            }
        }
        compose.onNodeWithContentDescription("A long profile name, Kids, PIN protected")
            .performSemanticsAction(SemanticsActions.OnLongClick)
        compose.runOnIdle {
            assertEquals(0, opened)
            assertEquals(1, edited)
        }
    }
}
