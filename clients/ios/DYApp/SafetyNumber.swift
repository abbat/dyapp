import CoreImage
import SwiftUI

/// The QR code of `payload`, six pixels to a module so a camera can read it from a screen.
func safetyQRImage(_ payload: Data) -> CGImage? {
    let filter = CIFilter(
        name: "CIQRCodeGenerator",
        parameters: ["inputMessage": payload, "inputCorrectionLevel": "M"]
    )
    guard let code = filter?.outputImage?.transformed(by: CGAffineTransform(scaleX: 6, y: 6)) else {
        return nil
    }
    return CIContext().createCGImage(code, from: code.extent)
}

/// What the two people compare out of band. The digits cannot be selected: a copy has to be
/// marked sensitive and cleared after 60 seconds, which only the desktop app does so far.
struct SafetyNumberSheet: View {
    @Environment(\.dismiss) private var dismiss
    let me: String?
    let peer: String

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("safety.action").font(.headline)
            if let me, let number = safetyNumberText(mine: me, theirs: peer) {
                Text(verbatim: number)
                    .font(.system(.body, design: .monospaced))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("safety-number")
                if let payload = safetyQrPayload(mine: me, theirs: peer),
                    let code = safetyQRImage(payload)
                {
                    // White whatever the theme: a scanner needs the light margin around the code.
                    Image(code, scale: 1, label: Text("safety.qr"))
                        .interpolation(.none)
                        .padding(24)
                        .background(Color.white)
                        .accessibilityIdentifier("safety-qr")
                }
                Text("safety.compare").font(.caption)
            } else {
                Text("safety.failed").font(.caption).accessibilityIdentifier("safety-error")
            }
            HStack {
                Spacer()
                Button("safety.done") { dismiss() }.accessibilityIdentifier("safety-done")
            }
        }
        .padding()
        .frame(minWidth: 320)
    }
}
