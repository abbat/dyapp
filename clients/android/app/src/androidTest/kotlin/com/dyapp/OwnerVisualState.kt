package com.dyapp

internal const val OWNER_VISUAL_IDENTITY = "2efa429a891a14eb7c79d3f55845ff59cfe53f70da52d61e1e0982fb683f9c2a"
internal const val OWNER_VISUAL_CODE = "dyapp-link:1:" +
    "9f2c41d07be3a65e18c4f07d2ba96e3150c8d7f4a1e62b90d3f75c18ae406b72:" +
    "2efa429a891a14eb7c79d3f55845ff59cfe53f70da52d61e1e0982fb683f9c2a:1798765432"

private const val VISUAL_INPUT = "owner-code-input"
private const val VISUAL_TARGET = "owner-recipient"
private const val VISUAL_UNCERTAIN = "owner-uncertain"
private const val VISUAL_EXPIRED = "owner-expired"

internal enum class OwnerVisualState(val screen: String, vararg tags: String) {
    INPUT("owner-code", VISUAL_INPUT),
    MALFORMED("owner-malformed", VISUAL_INPUT, "owner-malformed"),
    EXPIRED("owner-expired", VISUAL_INPUT, VISUAL_EXPIRED),
    PREPARING("owner-preparing", VISUAL_INPUT, "owner-pending"),
    PREPARE_ERROR("owner-prepare-error", VISUAL_INPUT, "owner-error"),
    CONFIRM("owner-confirm", VISUAL_TARGET),
    CONFIRM_EXPIRED("owner-confirm-expired", VISUAL_TARGET, VISUAL_EXPIRED),
    SENDING("owner-sending", VISUAL_TARGET, "owner-pending"),
    UNCERTAIN("owner-send-error", VISUAL_TARGET, VISUAL_UNCERTAIN),
    UNCERTAIN_EXPIRED("owner-send-expired", VISUAL_TARGET, VISUAL_UNCERTAIN, VISUAL_EXPIRED),
    SENT("owner-sent", VISUAL_TARGET, "owner-sent"),
    UNCERTAIN_CHECK_ERROR("owner-send-check-error", VISUAL_TARGET, VISUAL_UNCERTAIN, "owner-error");

    val facts = tags.toList()
}
