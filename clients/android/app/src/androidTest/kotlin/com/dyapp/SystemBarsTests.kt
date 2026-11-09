package com.dyapp

import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain

class SystemBarsTests {
    val compose = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    @Test
    fun systemBarsDoNotCoverPageControls() {
        compose.shownPeerId()
        val bars = compose.runOnIdle {
            val decor = compose.activity.window.decorView
            requireNotNull(ViewCompat.getRootWindowInsets(decor))
                .getInsets(WindowInsetsCompat.Type.systemBars())
        }
        val title = compose.onNodeWithTag(APP_READY_TAG).fetchSemanticsNode().boundsInRoot
        val navigation = compose.onNodeWithTag("nav-profile").fetchSemanticsNode().boundsInRoot
        val root = compose.onRoot().fetchSemanticsNode().size
        check(title.top >= bars.top) { "The status bar covers the page title" }
        check(navigation.bottom <= root.height - bars.bottom) { "The navigation bar covers the tab" }
    }
}
