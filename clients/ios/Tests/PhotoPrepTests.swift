@testable import DYApp
import ImageIO
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
        XCTAssertLessThanOrEqual(photo.full.count, 1_024 * 1_024)
    }

    /// ImageIO may add EXIF of its own; nothing named so may reach the network.
    func testTheFullImageIsALimitedJpegWithoutExif() throws {
        let png = try PhotoPrep.samplePNG(width: 3_000, height: 1_000, red: 0.5, alpha: 1)
        let photo = try PhotoPrep.prepare(png)
        XCTAssertEqual(photo.full.prefix(2), Data([0xFF, 0xD8]))
        XCTAssertNil(photo.full.range(of: Data("Exif".utf8)))
        let source = try XCTUnwrap(CGImageSourceCreateWithData(photo.full as CFData, nil))
        let metadata = try XCTUnwrap(
            CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [String: Any])
        XCTAssertEqual(metadata[kCGImagePropertyPixelWidth as String] as? Int, Int(photo.width))
        XCTAssertEqual(metadata[kCGImagePropertyPixelHeight as String] as? Int, Int(photo.height))
        XCTAssertLessThan(Int(photo.width), 3_000)
        XCTAssertEqual(try checkPreview(bytes: photo.thumbnail).width, 320)
    }

    func testChatPreparationUsesItsOwnDecodePlanAndFlattensPartialAlpha() throws {
        let png = try PhotoPrep.samplePNG(width: 3_000, height: 1_000, red: 1, alpha: 0.5)
        let photo = try PhotoPrep.prepare(png, purpose: .chat)
        XCTAssertEqual(photo.width, 2_560)
        let pixels = try checkPreview(bytes: photo.thumbnail).pixels
        XCTAssertGreaterThan(pixels[0], 240)
        XCTAssertTrue((115...140).contains(pixels[1]))
        XCTAssertTrue((115...140).contains(pixels[2]))
    }

    func testExifRotationIsAppliedBeforeSharedEncoding() throws {
        let png = try PhotoPrep.samplePNG(width: 3_000, height: 1_000, red: 1, alpha: 1)
        let source = try XCTUnwrap(CGImageSourceCreateWithData(png as CFData, nil))
        let image = try XCTUnwrap(CGImageSourceCreateImageAtIndex(source, 0, nil))
        let bytes = NSMutableData()
        let target = try XCTUnwrap(
            CGImageDestinationCreateWithData(bytes as CFMutableData, "public.jpeg" as CFString, 1, nil))
        CGImageDestinationAddImage(
            target, image, [kCGImagePropertyOrientation: 6] as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(target))

        let photo = try PhotoPrep.prepare(bytes as Data, purpose: .chat)
        XCTAssertEqual(photo.height, 2_560)
        XCTAssertGreaterThan(photo.height, photo.width)
        let upright = try XCTUnwrap(CGImageSourceCreateWithData(photo.full as CFData, nil))
        let output = try XCTUnwrap(CGImageSourceCreateImageAtIndex(upright, 0, nil))
        XCTAssertEqual(output.width, Int(photo.width))
        XCTAssertEqual(output.height, Int(photo.height))
    }

    func testAFullPeerPhotoMustBeAJpeg() throws {
        let png = try PhotoPrep.samplePNG(width: 8, height: 8, red: 1, alpha: 1)
        XCTAssertNil(fullPhotoImage(png))
        XCTAssertNotNil(fullPhotoImage(png, allowPNG: true))
        XCTAssertNotNil(fullPhotoImage(try PhotoPrep.prepare(png).full))
    }

    func testCoreCodesPickTheirText() {
        XCTAssertEqual(
            photoErrorKey(CoreError.Failed(message: "photoRefused")), "profile.photoRefused")
        XCTAssertEqual(
            photoErrorKey(CoreError.Failed(message: "corrupted")), "profile.photoUnsupported")
        XCTAssertEqual(photoErrorKey(CoreError.Failed(message: "disk full")), "profile.photoFailed")
        XCTAssertEqual(photoErrorKey(PhotoProblem.tooLarge), "profile.photoTooLarge")
    }
}
