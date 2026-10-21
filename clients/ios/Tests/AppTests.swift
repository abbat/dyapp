import XCTest

final class AppTests: XCTestCase {
    func testBuiltApplicationIdentity() {
        XCTAssertEqual(Bundle.main.bundleIdentifier, "com.dyapp.ios")
        XCTAssertEqual(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String, "0.0.1")
    }
}
