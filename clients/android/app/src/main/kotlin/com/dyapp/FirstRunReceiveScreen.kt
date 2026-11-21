package com.dyapp

import androidx.activity.compose.BackHandler
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue

/** Read actual active settings after the activation callback; a saved UI step never proves readiness. */
@Composable
internal fun FirstRunReceiveScreen(source: Opened.Ready, navigation: LinkReceiveNavigation) {
    var revision by rememberSaveable(source.slot.id, source.peerId) { mutableIntStateOf(0) }
    val needed = key(revision) { rememberNetworkEntryNeeded(source) }
    BackHandler(onBack = navigation.back)
    when (needed) {
        true -> NetworkReceiveWizard(source, navigation.back) { revision++ }
        false -> Page(actions = {}) {
            PageTitle(R.string.link_receive_title)
            LinkReceiveScreen(LinkReceiveSource(source, wizard = true), navigation)
        }
        null -> Page(actions = {}) {
            PageTitle(R.string.link_receive_network_title)
            CircularProgressIndicator()
        }
    }
}
