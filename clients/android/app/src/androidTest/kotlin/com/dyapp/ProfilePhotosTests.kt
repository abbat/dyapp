package com.dyapp

import android.graphics.Bitmap
import android.graphics.Color
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import java.io.ByteArrayOutputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import uniffi.dyapp.addProfilePhoto
import uniffi.dyapp.profilePhotos
import uniffi.dyapp.removeProfilePhoto

class ProfilePhotosTests {
    val compose = createAndroidComposeRule<MainActivity>()

    // Start past first-launch setup; LockSetupTests cover it.
    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(lockChoice(skipped = true)).around(compose)

    private fun string(id: Int, vararg args: Any) = compose.activity.getString(id, *args)

    private fun opened() = RustBridge.slots(compose.activity).open() as Opened.Ready

    private fun hashes() = RustBridge.withKey(opened()) { dir, key -> profilePhotos(dir, key) }.map { it.toList() }

    // A picked PNG with transparency, so the preparation flattens it as well.
    private fun png(color: Int): ByteArray {
        val bitmap = Bitmap.createBitmap(640, 480, Bitmap.Config.ARGB_8888).apply { eraseColor(color) }
        return ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
    }

    // The identity outlives a test, so earlier photos are dropped before the seed.
    private fun seed(count: Int) = RustBridge.withKey(opened()) { dir, key ->
        repeat(profilePhotos(dir, key).size) { removeProfilePhoto(dir, key, 0u) }
        repeat(count) {
            val photo = preparePhoto(png(Color.argb(128, it * 40, 0, 0)))
            addProfilePhoto(dir, key, photo.full, photo.thumbnail)
        }
    }

    @Test
    fun aGifIsRefusedBeforeTheCore() {
        val gif = "GIF89a".toByteArray() + ByteArray(32)
        val problem = assertThrows(PhotoProblem::class.java) { preparePhoto(gif) }
        assertEquals(R.string.profile_photo_unsupported, problem.text)
    }

    @Test
    fun sixPhotosDisableAddAndTheEditorMovesAndRemovesThem() {
        compose.skipNetworkEntry()
        seed(MAX_PROFILE_PHOTOS)
        compose.onNodeWithTag("nav-profile").performClick()
        compose.waitUntil(READBACK_WAIT_MS) {
            compose.onAllNodes(hasTestTag("profile-photo-5")).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithTag("profile-photo-add").performScrollTo().assertIsNotEnabled()
        compose.onNodeWithContentDescription(string(R.string.profile_photo_label, 1, 6)).assertIsDisplayed()

        val before = hashes()
        compose.onNodeWithContentDescription(string(R.string.profile_photo_left, 2)).performClick()
        compose.waitUntil(READBACK_WAIT_MS) { hashes() != before }
        assertEquals(listOf(before[1], before[0]) + before.drop(2), hashes())
        compose.waitForText("profile-photo-status", R.string.profile_photo_changed)

        compose.onNodeWithContentDescription(string(R.string.profile_photo_remove, 1)).performClick()
        compose.onNodeWithTag("profile-photo-remove-confirm").performClick()
        compose.waitUntil(READBACK_WAIT_MS) { hashes().size == MAX_PROFILE_PHOTOS - 1 }
        assertEquals(listOf(before[0]) + before.drop(2), hashes())
        compose.waitForText("profile-photo-status", R.string.profile_photo_changed)
        compose.onNodeWithTag("profile-photo-add").performScrollTo().assertIsEnabled()
        compose.onNodeWithContentDescription(string(R.string.profile_photo_label, 5, 5)).assertExists()
    }

    // The shell read the draft before the seed, so the avatar shows the photo only after an edit.
    @Test
    fun theOwnAvatarShowsTheMainPhotoAfterAnEdit() {
        compose.skipNetworkEntry()
        seed(2)
        compose.onNodeWithTag("nav-profile").performClick()
        compose.waitUntil(READBACK_WAIT_MS) {
            compose.onAllNodes(hasTestTag("profile-photo-1")).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithContentDescription(string(R.string.profile_photo_remove, 2)).performClick()
        compose.onNodeWithTag("profile-photo-remove-confirm").performClick()
        val avatar = hasTestTag("avatar-photo") and hasAnyAncestor(hasTestTag("identity-switcher"))
        // The switcher button merges its content, so the photo is found in the unmerged tree.
        compose.waitUntil(READBACK_WAIT_MS) {
            compose.onAllNodes(avatar, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
        }
    }
}
