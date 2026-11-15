import Foundation

struct RevealDrag {
    let start: CGPoint
    let end: CGPoint
}

/// The gesture begins on an actual noneditable label and stays in the current visible viewport.
func revealDrag(
    anchors: [CGRect], visible: CGRect, target: CGRect?, above: Bool
) -> RevealDrag? {
    guard !visible.isEmpty else { return nil }
    let down = target.map { $0.midY < visible.midY } ?? above
    let candidates = anchors.filter { !$0.isEmpty && visible.contains($0) }
    let anchor =
        down
        ? candidates.min { $0.midY < $1.midY }
        : candidates.max { $0.midY < $1.midY }
    guard let anchor else { return nil }
    let start = CGPoint(x: anchor.midX, y: anchor.midY)
    let available = (down ? visible.maxY : visible.minY) - start.y
    let wanted = target.map { visible.midY - $0.midY } ?? available
    let distance = down ? min(wanted, available) : max(wanted, available)
    guard down ? distance > 0 : distance < 0 else { return nil }
    return RevealDrag(start: start, end: CGPoint(x: start.x, y: start.y + distance))
}

func revealFits(_ frame: CGRect, in visible: CGRect) -> Bool {
    guard !frame.isEmpty && !visible.isEmpty else { return false }
    if frame.height > visible.height || frame.width > visible.width {
        return visible.contains(CGPoint(x: frame.midX, y: frame.midY))
    }
    return visible.contains(frame)
}
