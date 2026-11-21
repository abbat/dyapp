package com.dyapp

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import uniffi.dyapp.MobileQrMatrix

private const val QR_QUIET_MODULES = 4

/** Paints core-provided modules at integer pixel boundaries with the standard quiet zone. */
@Composable
internal fun LinkQr(matrix: MobileQrMatrix) {
    val description = stringResource(R.string.link_receive_qr)
    Canvas(
        Modifier.widthIn(max = 320.dp).fillMaxWidth().aspectRatio(1f).testTag("link-receive-qr")
            .semantics { contentDescription = description }
    ) {
        drawRect(Color.White)
        val side = matrix.size + QR_QUIET_MODULES * 2
        val pixels = (size.minDimension / side).toInt().toFloat()
        val left = ((size.width - side * pixels) / 2).toInt().toFloat()
        val top = ((size.height - side * pixels) / 2).toInt().toFloat()
        matrix.modules.forEachIndexed { index, dark ->
            if (dark) {
                val x = (index % matrix.size + QR_QUIET_MODULES) * pixels + left
                val y = (index / matrix.size + QR_QUIET_MODULES) * pixels + top
                drawRect(Color.Black, Offset(x, y), Size(pixels, pixels))
            }
        }
    }
}
