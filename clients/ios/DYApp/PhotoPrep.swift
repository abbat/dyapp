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
    let width: UInt32
    let height: UInt32
}

/// Turns a picked file into the bare JPEGs a profile publishes. ImageIO may write EXIF of its own,
/// ImageIO decodes orientation and sRGB pixels. The shared core prepares the published JPEGs.
enum PhotoPrep {
    /// Reads a picked file while holding access to the system selection.
    static func read(_ url: URL, limit: UInt64 = photoPickedByteLimit()) throws -> Data {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let file = try FileHandle(forReadingFrom: url)
        defer { try? file.close() }
        var bytes = Data()
        while let chunk = try file.read(upToCount: 64 * 1_024), !chunk.isEmpty {
            guard UInt64(bytes.count) + UInt64(chunk.count) <= limit else {
                throw PhotoProblem.tooLarge
            }
            bytes.append(chunk)
        }
        return bytes
    }

    static func prepare(_ bytes: Data, purpose: PhotoPurpose = .profile) throws -> PreparedPhoto {
        let image = try decoded(bytes, purpose: purpose)
        let photo = try preparePhotoPixels(
            rgba: straightPixels(image), width: UInt32(image.width), height: UInt32(image.height),
            purpose: purpose)
        return PreparedPhoto(
            full: photo.full, thumbnail: photo.thumbnail, width: photo.width, height: photo.height)
    }

    static func decoded(_ bytes: Data, purpose: PhotoPurpose) throws -> CGImage {
        guard UInt64(bytes.count) <= photoPickedByteLimit() else { throw PhotoProblem.tooLarge }
        let lazily = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithData(bytes as CFData, lazily),
            let identifier = CGImageSourceGetType(source) as String?,
            let mime = UTType(identifier)?.preferredMIMEType,
            let header = CGImageSourceCopyPropertiesAtIndex(source, 0, lazily) as? [String: Any],
            let width = header[kCGImagePropertyPixelWidth as String] as? UInt32,
            let height = header[kCGImagePropertyPixelHeight as String] as? UInt32
        else { throw PhotoProblem.unsupported }
        let plan = try photoDecodePlan(
            mime: mime, width: width, height: height, bytes: UInt64(bytes.count), purpose: purpose)
        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: Int(max(plan.width, plan.height)),
        ] as CFDictionary
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options),
            max(image.width, image.height) <= Int(max(plan.width, plan.height))
        else { throw PhotoProblem.unsupported }
        return image
    }

    /// Adapt Core Graphics' premultiplied RGBA layout to the core's straight RGBA input.
    private static func straightPixels(_ image: CGImage) throws -> Data {
        let width = image.width
        let height = image.height
        let bitmapInfo = CGBitmapInfo.byteOrder32Big.rawValue
            | CGImageAlphaInfo.premultipliedLast.rawValue
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
            let canvas = CGContext(
                data: nil, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: space, bitmapInfo: bitmapInfo),
            let data = canvas.data
        else { throw PhotoProblem.unsupported }
        canvas.setBlendMode(.copy)
        canvas.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        let pixels = data.assumingMemoryBound(to: UInt8.self)
        let count = width * height * 4
        for offset in stride(from: 0, to: count, by: 4) {
            let alpha = Int(pixels[offset + 3])
            guard alpha > 0, alpha < 255 else { continue }
            for channel in offset..<(offset + 3) {
                pixels[channel] = UInt8(min(255, (Int(pixels[channel]) * 255 + alpha / 2) / alpha))
            }
        }
        return Data(bytes: pixels, count: count)
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
