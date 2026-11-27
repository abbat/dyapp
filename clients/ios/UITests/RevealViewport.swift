import XCTest

struct RevealAnchor {
    let frame: CGRect
    let identity: String
}

@MainActor
func revealViewport(_ app: XCUIApplication) -> CGRect {
    let form =
        [app.collectionViews.firstMatch, app.tables.firstMatch, app.scrollViews.firstMatch]
        .first { $0.exists } ?? app
    var area = form.frame.intersection(app.frame)
    guard !area.isEmpty else { return .zero }
    let bar = app.navigationBars.firstMatch
    if bar.exists {
        area = area.intersection(
            CGRect(
                x: area.minX, y: bar.frame.maxY, width: area.width,
                height: max(0, area.maxY - bar.frame.maxY)))
    }
    guard !area.isEmpty else { return .zero }
    let keyboard = app.keyboards.firstMatch
    if keyboard.exists {
        area = area.intersection(
            CGRect(
                x: area.minX, y: area.minY, width: area.width,
                height: max(0, keyboard.frame.minY - area.minY)))
    }
    return area
}

/// Read one accessibility snapshot; labels and buttons are safe outside editable subtrees.
@MainActor
func revealAnchors(_ app: XCUIApplication, visible: CGRect) -> [RevealAnchor] {
    guard let snapshot = try? app.snapshot() else { return [] }
    func labels(_ node: any XCUIElementSnapshot) -> [RevealAnchor] {
        switch node.elementType {
        case .textView, .textField, .secureTextField:
            return []

        case .staticText, .button:
            guard !node.frame.isEmpty && visible.contains(node.frame) else { return [] }
            return [RevealAnchor(frame: node.frame, identity: "\(node.identifier)|\(node.label)")]

        default:
            return node.children.flatMap { labels($0) }
        }
    }
    return labels(snapshot).sorted {
        $0.frame.minY == $1.frame.minY
            ? $0.frame.minX < $1.frame.minX : $0.frame.minY < $1.frame.minY
    }
}
