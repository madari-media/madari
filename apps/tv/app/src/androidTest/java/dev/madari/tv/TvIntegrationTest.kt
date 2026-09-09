package dev.madari.tv

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.height
import androidx.compose.ui.unit.dp
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@OptIn(ExperimentalTestApi::class)
@RunWith(AndroidJUnit4::class)
class TvIntegrationTest {
    @get:Rule val compose = createComposeRule()

    @Test fun dpadMovesBetweenTvButtonsAndActivatesSelection() {
        val initial = FocusRequester()
        var selected = ""
        compose.setContent { MadariTheme {
            Row {
                Action("Home",{selected="Home"},Modifier.focusRequester(initial))
                Action("Search",{selected="Search"})
            }
        } }
        compose.runOnIdle { initial.requestFocus() }
        compose.onNodeWithText("Home").assertIsFocused().performKeyInput { pressKey(Key.DirectionRight) }
        compose.onNodeWithText("Search").assertIsFocused().performKeyInput { pressKey(Key.DirectionCenter) }
        compose.runOnIdle { assertEquals("Search",selected) }
    }

    @Test fun leftRailExpandsOnFocusAndReturnsToContent() {
        val initial=FocusRequester()
        var selected="Home"
        compose.setContent { MadariTheme {
            Row {
                androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.then(Modifier.width(250.dp))) {
                    NavigationRail("Home","Viewer",{selected=it},initial)
                }
                Action("Browse titles",{})
            }
        } }
        compose.runOnIdle { initial.requestFocus() }
        compose.onNodeWithContentDescription("Home").assertIsFocused()
        compose.onNodeWithText("Explore").assertExists()
        compose.onNodeWithContentDescription("Home").performKeyInput { pressKey(Key.DirectionDown) }
        compose.onNodeWithContentDescription("Explore").assertIsFocused().performKeyInput { pressKey(Key.DirectionCenter) }
        compose.runOnIdle { assertEquals("Explore",selected) }
        compose.onNodeWithText("Browse titles").assertIsFocused()
        compose.onNodeWithText("Explore").assertDoesNotExist()
    }

    @OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
    @Test fun focusScrollKeepsVisibleButtonsAndRevealsClippedItems() {
        assertEquals(0f,TvFocusScroll.calculateScrollDistance(220f,48f,540f),.01f)
        assertEquals(0f,TvFocusScroll.calculateScrollDistance(0f,414f,540f),.01f)
        assertEquals(0f,TvFocusScroll.calculateScrollDistance(-50f,700f,540f),.01f)
        assertTrue(TvFocusScroll.calculateScrollDistance(510f,100f,540f)>70f)
        assertTrue(TvFocusScroll.calculateScrollDistance(-30f,100f,540f)<-30f)
    }

    @Test fun focusingMiddleButtonDoesNotScrollAwayHeader() {
        val focus=FocusRequester()
        val scroll=androidx.compose.foundation.lazy.LazyListState()
        compose.setContent { MadariTheme {
            androidx.compose.foundation.lazy.LazyColumn(Modifier.height(400.dp),state=scroll) {
                item { androidx.compose.foundation.layout.Box(Modifier.height(180.dp)) { Heading("Visible header") } }
                item { Action("Middle button",{},Modifier.focusRequester(focus)) }
                item { androidx.compose.foundation.layout.Spacer(Modifier.height(600.dp)) }
            }
        } }
        compose.runOnIdle { focus.requestFocus() }
        compose.waitForIdle()
        compose.onNodeWithText("Visible header").assertIsDisplayed()
        compose.runOnIdle { assertEquals(0,scroll.firstVisibleItemIndex); assertEquals(0,scroll.firstVisibleItemScrollOffset) }
    }

    @Test fun jniProfilesLibraryProgressAndPlaybackRoundTrip() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        NativeCore.initializeTls(context)
        NativeCore.initialize(context.cacheDir.resolve("native-test-${System.nanoTime()}").absolutePath)
        fun call(operation: String, args: JSONObject = JSONObject()) = NativeCore.dispatch(operation,args.toString())
        val profile=JSONObject(call("create_profile",obj("name" to "Test TV","pin" to "1234")))
        call("unlock",obj("id" to profile.text("id"),"pin" to "1234"))
        val key=obj("installation_id" to "test","content_type" to "movie","item_id" to "movie")
        val meta=obj("id" to "movie","type" to "movie","name" to "Test movie")
        call("save",obj("key" to key,"metadata" to meta,"title" to "Test movie"))
        call("progress",obj("key" to key,"metadata" to meta,"video_id" to "movie","position_ms" to 15000,"duration_ms" to 120000,"completed" to false))
        val snapshot=JSONObject(call("snapshot"))
        assertEquals("Test movie",snapshot.getJSONArray("library").getJSONObject(0).getString("title"))
        val prepared=JSONObject(call("prepare",obj("key" to key,"video_id" to "movie","source" to obj("url" to "https://example.com/movie.mp4"),"capabilities" to obj("url_schemes" to org.json.JSONArray(listOf("https"))))))
        assertEquals(15000,prepared.getJSONObject("plan").getLong("resume_ms"))
        assertEquals("direct",prepared.getJSONObject("delivery").getString("kind"))
    }
}
