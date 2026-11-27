import Foundation
import XCTest

/// A one-shot callback signal with a fresh failure deadline for each awaited transition.
final class NativeTestSignal: @unchecked Sendable {
    private let event: XCTestExpectation
    private let lock = NSLock()
    private var completed = false

    init(_ description: String = "Native transition") {
        event = XCTestExpectation(description: description)
    }

    func signal() {
        lock.lock()
        defer { lock.unlock() }
        guard !completed else { return }
        completed = true
        event.fulfill()
    }

    func wait() async -> Bool {
        await withCheckedContinuation { continuation in
            DispatchQueue.global().async { [self] in
                continuation.resume(
                    returning: XCTWaiter.wait(for: [event], timeout: 10) == .completed)
            }
        }
    }
}
