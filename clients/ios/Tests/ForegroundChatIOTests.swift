@testable import DYApp
import XCTest

@MainActor
final class ForegroundChatIOTests: XCTestCase {
    func testReadObservesPreviouslyAdmittedWrite() async throws {
        let root = try identitySlotTestRoot()
        let file = root.appendingPathComponent("ordered-write")
        let entered = NativeTestSignal("Write admitted")
        let release = DispatchSemaphore(value: 0)
        let expected = Data("committed".utf8)
        let write = Task {
            await ForegroundChatIO.perform {
                entered.signal()
                guard release.wait(timeout: .now() + 10) == .success else { return false }
                return (try? expected.write(to: file)) != nil
            }
        }
        defer { release.signal() }
        let started = await entered.wait()
        guard started else { return XCTFail("Write never entered native IO") }
        // Releasing the worker lets it commit. The following read must observe that commit.
        release.signal()
        let observed = await ForegroundChatIO.perform { try? Data(contentsOf: file) }
        XCTAssertEqual(observed, expected)
        let wrote = await write.value
        XCTAssertTrue(wrote)
    }

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
