@testable import DYApp
import XCTest

final class IdentityStoreCacheTests: XCTestCase {
    func testConcurrentCallersShareOneNativeRegistryWithoutCreatingAnIdentity() throws {
        let root = try identitySlotTestRoot()
        let cache = IdentityStoreCache()
        let legacy = PlatformKeyStore.forSlot(UUID())
        let first = cache.store(root: root, legacyStore: legacy)
        DispatchQueue.concurrentPerform(iterations: 16) { _ in
            XCTAssertIdentical(
                cache.store(root: root, legacyStore: legacy).registry, first.registry)
        }
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), [])
        XCTAssertEqual(try first.resolve(), .setup)
    }

    func testDifferentRootsKeepIndependentViewsAndSelection() throws {
        let firstRoot = try identitySlotTestRoot()
        let secondRoot = try identitySlotTestRoot()
        let cache = IdentityStoreCache()
        let first = cache.store(root: firstRoot, legacyStore: .forSlot(UUID()))
        let second = cache.store(root: secondRoot, legacyStore: .forSlot(UUID()))
        XCTAssertNotIdentical(first.registry, second.registry)
        let created = try first.create()
        XCTAssertEqual(try first.resolve(), created.update.resolution)
        XCTAssertEqual(try second.resolve(), .setup)
        XCTAssertIdentical(cache.store(root: firstRoot, legacyStore: .shared), first)
    }
}
