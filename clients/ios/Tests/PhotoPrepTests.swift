@testable import DYApp
import XCTest

final class PhotoPrepTests: XCTestCase {
    func testAGifIsRefusedBeforeTheCore() {
        let gif = Data("GIF89a".utf8) + Data(count: 32)
        XCTAssertThrowsError(try PhotoPrep.prepare(gif)) { error in
            XCTAssertEqual(photoErrorKey(error), "profile.photoUnsupported")
        }
    }

    func testTransparencyFlattensOntoWhiteAndTheThumbnailFitsTheCore() throws {
        let png = try PhotoPrep.samplePNG(width: 640, height: 480, red: 1, alpha: 0)
        let photo = try PhotoPrep.prepare(png)
        let preview = try checkPreview(bytes: photo.thumbnail)
        XCTAssertEqual(preview.width, 320)
        XCTAssertEqual(preview.height, 240)
        XCTAssertGreaterThan(preview.pixels[1], 240, "green of the first pixel")
        XCTAssertLessThanOrEqual(photo.full.count, 1024 * 1024)
    }

    /// ImageIO may add EXIF of its own; nothing named so may reach the network.
    func testTheFullImageIsALimitedJpegWithoutExif() throws {
        let png = try PhotoPrep.samplePNG(width: 3000, height: 1000, red: 0.5, alpha: 1)
        let photo = try PhotoPrep.prepare(png)
        XCTAssertEqual(photo.full.prefix(2), Data([0xFF, 0xD8]))
        XCTAssertNil(photo.full.range(of: Data("Exif".utf8)))
        XCTAssertEqual(try checkPreview(bytes: photo.thumbnail).width, 320)
    }

    func testCoreCodesPickTheirText() {
        XCTAssertEqual(photoErrorKey(CoreError.Failed(message: "photoRefused")), "profile.photoRefused")
        XCTAssertEqual(photoErrorKey(CoreError.Failed(message: "corrupted")), "profile.photoUnsupported")
        XCTAssertEqual(photoErrorKey(CoreError.Failed(message: "disk full")), "profile.photoFailed")
        XCTAssertEqual(photoErrorKey(PhotoProblem.tooLarge), "profile.photoTooLarge")
    }
}
