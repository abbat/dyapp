package com.dyapp

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.activity.result.ActivityResultRegistry
import androidx.activity.result.contract.ActivityResultContract
import androidx.core.app.ActivityOptionsCompat
import org.junit.Assert.assertEquals
import java.util.concurrent.CopyOnWriteArrayList

/** Deliver explicit real contract results without launching an external provider UI. */
internal class SavedStickerPickerRegistry(private val context: Context) : ActivityResultRegistry() {
    val requests = CopyOnWriteArrayList<Int>()

    override fun <I, O> onLaunch(
        requestCode: Int,
        contract: ActivityResultContract<I, O>,
        input: I,
        options: ActivityOptionsCompat?
    ) {
        val intent = contract.createIntent(context, input)
        assertEquals(Intent.ACTION_OPEN_DOCUMENT, intent.action)
        assertEquals("*/*", intent.type)
        requests.add(requestCode)
    }

    fun deliver(code: Int, uri: Uri) {
        dispatchResult(code, Activity.RESULT_OK, Intent().setData(uri))
    }
}
