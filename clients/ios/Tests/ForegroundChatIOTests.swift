@testable import DYApp
import XCTest

@MainActor
final class ForegroundChatIOTests: XCTestCase {
    func testCancellingCallerStillDeliversActualWorkerResult() async {
        let entered = NativeTestSignal("Foreground write entered")
        let finished = NativeTestSignal("Foreground write finished")
        let release = DispatchSemaphore(value: 0)
        let write = Task {
            let result = await ForegroundChatIO.perform {
                entered.signal()
                return release.wait(timeout: .now() + 10) == .success
            }
            finished.signal()
            return result
        }
        defer { release.signal() }
        let started = await entered.wait()
        guard started else { return XCTFail("Write never entered native IO") }
        write.cancel()
        release.signal()
        let completed = await finished.wait()
        guard completed else { return XCTFail("Write did not complete") }
        let wrote = await write.value
        XCTAssertTrue(wrote)
    }
}
