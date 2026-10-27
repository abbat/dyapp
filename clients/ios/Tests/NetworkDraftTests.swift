@testable import DYApp
import Foundation
import XCTest

final class NetworkDraftTests: XCTestCase {
    private func root() throws -> URL {
        let path = FileManager.default.temporaryDirectory
            .appendingPathComponent("dyapp-network-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: path, withIntermediateDirectories: false)
        return path
    }

    private func folder(root: URL, id: UUID) throws -> URL {
        let candidate = IdentityCandidate(location: .slot(id), state: .ready)
        let path = RustBridge.networkDirectory(root: root, candidate: candidate)
        try FileManager.default.createDirectory(at: path, withIntermediateDirectories: true)
        return path
    }

    func testDraftsKeepOrderAndStayInTheirIdentityFolders() throws {
        let root = try root()
        let first = try folder(root: root, id: UUID())
        let second = try folder(root: root, id: UUID())
        let firstDraft = NetworkSettings(seeds: ["one.example:33445", "two.example:7070"], name: "friends")
        let secondDraft = NetworkSettings(seeds: ["other.example:33445"], name: "")

        XCTAssertNil(try loadNetwork(dir: first.path, file: .draft))
        XCTAssertNil(try loadNetwork(dir: second.path, file: .draft))
        try saveNetworkDraft(dir: first.path, draft: firstDraft)
        XCTAssertNil(try loadNetwork(dir: second.path, file: .draft))
        try saveNetworkDraft(dir: second.path, draft: secondDraft)

        let reopenedFirst = try XCTUnwrap(loadNetwork(dir: first.path, file: .draft))
        let reopenedSecond = try XCTUnwrap(loadNetwork(dir: second.path, file: .draft))
        XCTAssertEqual(reopenedFirst.seeds, firstDraft.seeds)
        XCTAssertEqual(reopenedFirst.name, firstDraft.name)
        XCTAssertEqual(reopenedSecond.seeds, secondDraft.seeds)
        XCTAssertEqual(reopenedSecond.name, secondDraft.name)
        XCTAssertNil(try loadNetwork(dir: first.path, file: .active))
    }

    func testActiveAndDraftStayInTheirIdentityAfterReopen() throws {
        let root = try root()
        let firstID = UUID()
        let secondID = UUID()
        let first = try folder(root: root, id: firstID)
        let second = try folder(root: root, id: secondID)
        let firstSettings = NetworkSettings(seeds: ["first.example:33445"], name: "first")
        let secondSettings = NetworkSettings(seeds: ["second.example:33445"], name: "second")
        try saveNetworkDraft(dir: first.path, draft: firstSettings)
        try saveNetworkDraft(dir: second.path, draft: secondSettings)
        try Data(contentsOf: first.appendingPathComponent("network-draft"))
            .write(to: first.appendingPathComponent("network"))

        XCTAssertNil(try loadNetwork(dir: second.path, file: .active))
        try Data(contentsOf: second.appendingPathComponent("network-draft"))
            .write(to: second.appendingPathComponent("network"))
        let reopenedFirst = try folder(root: root, id: firstID)
        let reopenedSecond = try folder(root: root, id: secondID)
        for file in [NetworkFile.draft, .active] {
            let firstRead = try XCTUnwrap(loadNetwork(dir: reopenedFirst.path, file: file))
            let secondRead = try XCTUnwrap(loadNetwork(dir: reopenedSecond.path, file: file))
            XCTAssertEqual(firstRead.seeds, firstSettings.seeds)
            XCTAssertEqual(firstRead.name, firstSettings.name)
            XCTAssertEqual(secondRead.seeds, secondSettings.seeds)
            XCTAssertEqual(secondRead.name, secondSettings.name)
        }
    }

    func testDamagedDraftDoesNotChangeActiveSettingsOrIdentityFile() throws {
        let path = try folder(root: root(), id: UUID())
        let key = path.appendingPathComponent("identity.key")
        let keyBytes = Data("existing-key".utf8)
        try keyBytes.write(to: key)
        let settings = NetworkSettings(seeds: ["active.example:33445"], name: "public-test")
        try saveNetworkDraft(dir: path.path, draft: settings)
        let draft = path.appendingPathComponent("network-draft")
        let active = path.appendingPathComponent("network")
        try Data(contentsOf: draft).write(to: active)
        let damaged = Data("invalid network file\n".utf8)
        try damaged.write(to: draft, options: .atomic)

        XCTAssertThrowsError(try loadNetwork(dir: path.path, file: .draft))
        XCTAssertEqual(try Data(contentsOf: draft), damaged)
        XCTAssertEqual(try Data(contentsOf: key), keyBytes)
        let retained = try XCTUnwrap(loadNetwork(dir: path.path, file: .active))
        XCTAssertEqual(retained.seeds, settings.seeds)
        XCTAssertEqual(retained.name, settings.name)
    }

    func testConnectionSessionChecksOnlyItsDraftWithoutActivation() throws {
        let root = try root()
        let first = try folder(root: root, id: UUID())
        let second = try folder(root: root, id: UUID())
        let session = try NetworkConnectionSession(directory: first)

        XCTAssertEqual(try session.run(rows: ["not-an-address"], name: "", waitMs: 200),
                       [.invalidAddress])
        let draft = try XCTUnwrap(loadNetwork(dir: first.path, file: .draft))
        XCTAssertEqual(draft.seeds, ["not-an-address"])
        XCTAssertThrowsError(try session.activate())
        XCTAssertNil(try loadNetwork(dir: first.path, file: .active))
        XCTAssertNil(try loadNetwork(dir: second.path, file: .draft))
    }

    func testLocalNodeNeedsExplicitActivationAfterPinnedCheck() throws {
        let path = try folder(root: root(), id: UUID())
        let session = try NetworkConnectionSession(directory: path)
        let address = "127.0.0.1:33445"
        let unpinned = try session.run(rows: [address], name: "", waitMs: 5_000)
        guard case let .connected(peerID) = try XCTUnwrap(unpinned.first) else {
            XCTFail("The CI loopback node did not answer")
            return
        }
        let pinned = "\(address)/\(peerID)"
        XCTAssertEqual(try session.run(rows: [pinned], name: "", waitMs: 5_000),
                       [.connected(peerID)])
        XCTAssertNil(try loadNetwork(dir: path.path, file: .active))

        try session.activate()
        let active = try XCTUnwrap(loadNetwork(dir: path.path, file: .active))
        XCTAssertEqual(active.seeds, [pinned])
        XCTAssertEqual(active.name, "")
    }

    func testInvalidDuplicateAndUnreachableRowsStayOrdered() throws {
        let path = try folder(root: root(), id: UUID())
        let session = try NetworkConnectionSession(directory: path)
        let rows = [
            "not-an-address", "127.0.0.1:33446",
            "127.0.0.1:33446", "127.0.0.1:33445/bad",
        ]

        XCTAssertEqual(try session.run(rows: rows, name: "", waitMs: 1_000),
                       [.invalidAddress, .unreachable, .duplicate, .invalidPeerID])
        XCTAssertThrowsError(try session.activate())
        XCTAssertNil(try loadNetwork(dir: path.path, file: .active))
        XCTAssertEqual(try XCTUnwrap(loadNetwork(dir: path.path, file: .draft)).seeds, rows)
    }

    func testWrongPeerAndNetworkCannotActivate() throws {
        let path = try folder(root: root(), id: UUID())
        let session = try NetworkConnectionSession(directory: path)
        let address = "127.0.0.1:33445"
        let connected = try session.run(rows: [address], name: "", waitMs: 5_000)
        guard case let .connected(peerID) = try XCTUnwrap(connected.first) else {
            XCTFail("The CI loopback node did not answer")
            return
        }
        let wrongPeer = String(peerID.dropLast()) + (peerID.last == "1" ? "2" : "1")
        let wrongPinCheck = try NetworkConnectionSession(directory: path)
        XCTAssertEqual(try wrongPinCheck.run(rows: ["\(address)/\(wrongPeer)"], name: "", waitMs: 5_000),
                       [.wrongPeer])
        XCTAssertThrowsError(try wrongPinCheck.activate())
        let wrongNetworkCheck = try NetworkConnectionSession(directory: path)
        XCTAssertEqual(try wrongNetworkCheck.run(rows: [address], name: "city", waitMs: 5_000),
                       [.wrongNetwork])
        XCTAssertThrowsError(try wrongNetworkCheck.activate())
        XCTAssertNil(try loadNetwork(dir: path.path, file: .active))
    }

    func testCancelRetryKeepsPriorActiveNetwork() async throws {
        let path = try folder(root: root(), id: UUID())
        let working = try NetworkConnectionSession(directory: path)
        let good = "127.0.0.1:33445"
        let silent = "127.0.0.1:33447"
        guard case .connected = try XCTUnwrap(working.run(rows: [good], name: "", waitMs: 5_000).first) else {
            XCTFail("The CI loopback node did not answer")
            return
        }
        try working.activate()

        let checking = try NetworkConnectionSession(directory: path)
        let started = Date()
        let running = Task.detached { () -> Bool in
            do {
                _ = try checking.run(rows: [silent], name: "", waitMs: 30_000)
                return false
            } catch {
                return true
            }
        }
        let stopping = Task.detached {
            for _ in 0..<20 {
                try? await Task.sleep(nanoseconds: 100_000_000)
                checking.cancel()
            }
        }
        let cancelled = await running.value
        await stopping.value
        XCTAssertTrue(cancelled)
        XCTAssertLessThan(Date().timeIntervalSince(started), 10)
        XCTAssertEqual(try XCTUnwrap(loadNetwork(dir: path.path, file: .draft)).seeds, [silent])
        XCTAssertEqual(try XCTUnwrap(loadNetwork(dir: path.path, file: .active)).seeds, [good])

        let retry = try NetworkConnectionSession(directory: path)
        XCTAssertEqual(try retry.run(rows: [silent], name: "", waitMs: 500), [.unreachable])
        XCTAssertEqual(try XCTUnwrap(loadNetwork(dir: path.path, file: .active)).seeds, [good])
    }
}
