@testable import DYApp
import XCTest

final class AppTests: XCTestCase {
    func testBuiltApplicationIdentity() {
        XCTAssertEqual(Bundle.main.bundleIdentifier, "com.dyapp.macos")
        XCTAssertEqual(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String, "0.0.1")
    }

    func testStorageNumbersAreMeasuredOrUnknown() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        XCTAssertNil(StorageUsage.messageBytes(in: folder))
        try Data(count: 10).write(to: folder.appendingPathComponent("client.db"))
        XCTAssertEqual(StorageUsage.messageBytes(in: folder), 10)
        try Data(count: 5).write(to: folder.appendingPathComponent("client.db-wal"))
        XCTAssertEqual(StorageUsage.messageBytes(in: folder), 15)
        XCTAssertNotNil(StorageUsage.freeBytes(on: folder))
        XCTAssertNil(StorageUsage.freeBytes(on: folder.appendingPathComponent("gone")))
    }

    func testStorageSizesUseBinaryUnits() {
        let english = Locale(identifier: "en_US")
        XCTAssertTrue(StorageUsage.sizeText(0, locale: english).hasPrefix("0 "))
        XCTAssertTrue(StorageUsage.sizeText(1_023, locale: english).hasPrefix("1,023 "))
        XCTAssertTrue(StorageUsage.sizeText(1_536, locale: english).hasPrefix("1.5 "))
        XCTAssertTrue(StorageUsage.sizeText(1_048_576, locale: english).hasPrefix("1.0 "))
    }

    func testStatusBarNamesMeasuredBytesOrSaysUnavailable() throws {
        XCTAssertEqual(StatusBar.trafficText(nil), NSLocalizedString("statusbar.noTraffic", comment: ""))
        XCTAssertEqual(StatusBar.messagesText(nil), NSLocalizedString("statusbar.noMessages", comment: ""))
        let traffic = StatusBar.trafficText(ClientTraffic(received: 1_536, sent: 3_072))
        XCTAssertLessThan(
            try XCTUnwrap(traffic.range(of: StorageUsage.sizeText(1_536))).lowerBound,
            try XCTUnwrap(traffic.range(of: StorageUsage.sizeText(3_072))).lowerBound
        )
        XCTAssertTrue(StatusBar.messagesText(2_048).contains(StorageUsage.sizeText(2_048)))
    }

    func testASavedFrameStaysOnlyWhereItCanBeReached() {
        let primary = CGRect(x: 0, y: 0, width: 1_000, height: 800)
        let right = CGRect(x: 1_000, y: 0, width: 500, height: 800)
        let onBoth = CGRect(x: 800, y: 100, width: 400, height: 500)
        XCTAssertEqual(WindowPlacement.restored(onBoth, on: [primary, right]), onBoth)
        // A display went away: less than a quarter of the window is left.
        XCTAssertEqual(
            WindowPlacement.restored(CGRect(x: 950, y: 100, width: 400, height: 500), on: [primary]),
            CGRect(x: 300, y: 150, width: 400, height: 500)
        )
        // Most of the window shows, but its title bar is above the display.
        XCTAssertEqual(
            WindowPlacement.restored(CGRect(x: 100, y: 400, width: 400, height: 500), on: [primary]),
            CGRect(x: 300, y: 150, width: 400, height: 500)
        )
        XCTAssertEqual(
            WindowPlacement.restored(CGRect(x: 5_000, y: 0, width: 2_000, height: 1_000), on: [primary]),
            primary
        )
    }
}
