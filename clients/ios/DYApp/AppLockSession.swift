import Darwin

/// Owns the common timer and forwards platform events; no Swift expiry or unlock policy.
struct AppLockSession {
    private var timer: MobileAppLockTimer
    private var backgrounded = false

    init(now: UInt64) {
        timer = MobileAppLockTimer(enabled: true, nowMs: now)
    }

    static func uptime() -> UInt64 {
        clock_gettime_nsec_np(CLOCK_MONOTONIC) / 1_000_000
    }

    /// An observed stored mode may enable only a locked timer, never authorize a session.
    mutating func setProtection(_ enabled: Bool, at now: UInt64) throws {
        if enabled {
            if try !timer.snapshot(nowMs: now).enabled {
                timer = MobileAppLockTimer(enabled: true, nowMs: now)
                if backgrounded { _ = try timer.transition(event: .background, nowMs: now) }
            }
        } else {
            _ = try timer.transition(event: .cleared, nowMs: now)
        }
    }

    mutating func phase(active: Bool, at now: UInt64) throws {
        backgrounded = !active
        _ = try timer.transition(event: active ? .foreground : .background, nowMs: now)
    }

    func interaction(at now: UInt64) throws {
        _ = try timer.transition(event: .interaction, nowMs: now)
    }

    /// Call only after a successful PIN or fresh OS credential operation.
    func authorize(at now: UInt64) throws {
        _ = try timer.transition(event: .unlocked, nowMs: now)
    }

    func snapshot(at now: UInt64) throws -> MobileAppLockSnapshot {
        try timer.snapshot(nowMs: now)
    }
}
