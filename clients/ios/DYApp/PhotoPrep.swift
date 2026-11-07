import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

enum PhotoProblem: Error {
    case tooLarge
    case unsupported
}

struct PreparedPhoto: Sendable {
    let full: Data
    let thumbnail: Data
}

/// Turns a picked file into the bare JPEGs a profile publishes. ImageIO may write EXIF of its own,
/// so the core's `stripMetadata` has the last word on what leaves the device.
enum PhotoPrep {
    private static let maxPickedBytes = 64 * 1_024 * 1_024
    private static let maxPixels = 64_000_000
    private static let maxFullBytes = 1_024 * 1_024
    private static let maxThumbnailBytes: UInt32 = 24 * 1_024
    private static let thumbnailSide = 320
    private static let fullSides = [2_048, 1_600, 1_280]
    private static let fullQualities = [85, 70, 55]

    /// Reads a picked file, refusing an oversized one before its bytes are in memory.
    static func read(_ url: URL) throws -> Data {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        if let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize,
            size > maxPickedBytes
        {
            throw PhotoProblem.tooLarge
        }
        return try Data(contentsOf: url)
    }

    /// The first size and quality within the limit wins; the header is checked before any decoding.
    static func prepare(_ bytes: Data) throws -> PreparedPhoto {
        let upright = try decoded(bytes)
        for side in fullSides {
            guard let image = try flattened(upright, side: side).makeImage() else {
                throw PhotoProblem.unsupported
            }
            for quality in fullQualities {
                let full = try jpeg(image, quality: quality)
                if full.count <= maxFullBytes {
                    return PreparedPhoto(full: full, thumbnail: try thumbnail(upright))
                }
            }
        }
        throw PhotoProblem.tooLarge
    }

    private static func decoded(_ bytes: Data) throws -> CGImage {
        guard bytes.count <= maxPickedBytes else { throw PhotoProblem.tooLarge }
        let lazily = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithData(bytes as CFData, lazily),
            let type = CGImageSourceGetType(source) as String?,
            type == UTType.jpeg.identifier || type == UTType.png.identifier,
            let header = CGImageSourceCopyPropertiesAtIndex(source, 0, lazily) as? [String: Any],
            let width = header[kCGImagePropertyPixelWidth as String] as? Int,
            let height = header[kCGImagePropertyPixelHeight as String] as? Int,
            width > 0, height > 0
        else { throw PhotoProblem.unsupported }
        guard width * height <= maxPixels else { throw PhotoProblem.tooLarge }
        let options =
            [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceShouldCacheImmediately: true,
                kCGImageSourceThumbnailMaxPixelSize: fullSides[0],
            ] as CFDictionary
        guard let upright = CGImageSourceCreateThumbnailAtIndex(source, 0, options) else {
            throw PhotoProblem.unsupported
        }
        return upright
    }

    /// The image on white at most `side` px long, in sRGB with tightly packed RGBA rows.
    private static func flattened(_ image: CGImage, side: Int) throws -> CGContext {
        let longest = max(image.width, image.height)
        let width = longest <= side ? image.width : max(1, image.width * side / longest)
        let height = longest <= side ? image.height : max(1, image.height * side / longest)
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
            let canvas = CGContext(
                data: nil, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { throw PhotoProblem.unsupported }
        let rect = CGRect(x: 0, y: 0, width: width, height: height)
        canvas.setFillColor(red: 1, green: 1, blue: 1, alpha: 1)
        canvas.fill(rect)
        canvas.interpolationQuality = .high
        canvas.draw(image, in: rect)
        return canvas
    }

    private static func jpeg(_ image: CGImage, quality: Int) throws -> Data {
        let out = NSMutableData()
        guard
            let target = CGImageDestinationCreateWithData(
                out as CFMutableData,
                UTType.jpeg.identifier as CFString, 1, nil)
        else { throw PhotoProblem.unsupported }
        let options =
            [kCGImageDestinationLossyCompressionQuality: Double(quality) / 100] as CFDictionary
        CGImageDestinationAddImage(target, image, options)
        guard CGImageDestinationFinalize(target) else { throw PhotoProblem.unsupported }
        return try stripMetadata(bytes: out as Data)
    }

    private static func thumbnail(_ image: CGImage) throws -> Data {
        let canvas = try flattened(image, side: thumbnailSide)
        guard let pixels = canvas.data else { throw PhotoProblem.unsupported }
        let rgba = Data(bytes: pixels, count: canvas.width * canvas.height * 4)
        return try makePreview(
            rgba: rgba, width: UInt32(canvas.width), height: UInt32(canvas.height),
            maxBytes: maxThumbnailBytes)
    }

    #if DEBUG
        /// A flat PNG for tests, which cannot drive the system picker.
        static func samplePNG(width: Int, height: Int, red: CGFloat, alpha: CGFloat) throws -> Data
        {
            guard let space = CGColorSpace(name: CGColorSpace.sRGB),
                let canvas = CGContext(
                    data: nil, width: width, height: height, bitsPerComponent: 8,
                    bytesPerRow: width * 4, space: space,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { throw PhotoProblem.unsupported }
            canvas.setFillColor(red: red, green: 0, blue: 0, alpha: alpha)
            canvas.fill(CGRect(x: 0, y: 0, width: width, height: height))
            let out = NSMutableData()
            guard let image = canvas.makeImage(),
                let target = CGImageDestinationCreateWithData(
                    out as CFMutableData,
                    UTType.png.identifier as CFString, 1, nil)
            else { throw PhotoProblem.unsupported }
            CGImageDestinationAddImage(target, image, nil)
            guard CGImageDestinationFinalize(target) else { throw PhotoProblem.unsupported }
            return out as Data
        }
    #endif
}
