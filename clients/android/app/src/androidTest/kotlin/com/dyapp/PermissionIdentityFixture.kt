package com.dyapp

/** Own only the alternate identity created here; never erase or replace an existing profile. */
internal class PermissionIdentityFixture(private val slots: Slots, private val original: Opened.Ready) : AutoCloseable {
    private val initialIds = slots.list().map { it.id }.toSet()
    val replacement: Opened.Ready

    init {
        check(original.slot.id in initialIds) { "Original permission owner is not inventoried" }
        check((slots.open() as? Opened.Ready)?.peerId == original.peerId) {
            "Original permission owner is not selected"
        }
        replacement = slots.create()
        check(replacement.slot.id !in initialIds) { "Permission fixture reused an existing identity" }
        check(replacement.peerId != original.peerId) { "Permission fixture must use a distinct identity" }
        restoreOriginal()
    }

    fun selectReplacement() {
        verifyInventory()
        choose(replacement)
    }

    fun restoreOriginal() {
        verifyInventory()
        choose(original)
    }

    private fun choose(target: Opened.Ready) {
        val selected = slots.choose(target.slot.id)
        check(selected is Opened.Ready && selected.peerId == target.peerId) {
            "Permission fixture selected the wrong owner"
        }
    }

    private fun verifyInventory() {
        check(slots.list().map { it.id }.toSet() == initialIds + replacement.slot.id) {
            "Permission fixture inventory changed"
        }
        val inspected = slots.inspect(replacement.slot.id)
        check(inspected is Opened.Ready && inspected.peerId == replacement.peerId) {
            "Permission fixture ownership changed"
        }
    }

    override fun close() {
        restoreOriginal()
        slots.remove(replacement.slot.id, replacement.peerId)
        check(slots.list().map { it.id }.toSet() == initialIds) {
            "Permission fixture cleanup changed another identity"
        }
        check((slots.open() as? Opened.Ready)?.peerId == original.peerId) {
            "Permission fixture cleanup changed selection"
        }
    }
}
