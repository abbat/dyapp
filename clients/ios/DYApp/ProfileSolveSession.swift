import Foundation

struct ProfileWorkProgress: Sendable {
    let solved: Bool
    let cancelled: Bool
    let nextNonce: UInt64
}

enum ProfileWorkAttempt: Sendable {
    case progress(ProfileWorkProgress)
    case failed
}

/// UniFFI's Rust object synchronizes each step and accepts cancellation on another thread.
final class ProfileSolveSession: @unchecked Sendable {
    private let solver: ProfileSolve

    init(solver: ProfileSolve) {
        self.solver = solver
    }

    func runStep(attested: Bool) async -> ProfileWorkAttempt {
        await IdentityIO.performBlocking { [self] () -> ProfileWorkAttempt in
            do {
                let step = try RustBridge.withDraftLock { try solver.runStep(attested: attested) }
                return .progress(
                    ProfileWorkProgress(
                        solved: step.solved, cancelled: step.cancelled,
                        nextNonce: step.nextNonce))
            } catch {
                return .failed
            }
        }
    }

    func cancel() {
        solver.cancel()
    }
}
