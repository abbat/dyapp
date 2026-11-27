import Foundation

func chatListPreview(_ preview: ChatListPreview) -> String {
    let text: String
    switch preview.body {
    case .empty:
        return NSLocalizedString("chats.noMessages", comment: "")

    case .deleted:
        return NSLocalizedString("chats.deleted", comment: "")

    case .text(let value):
        text = value

    case .photo:
        text = NSLocalizedString("chat.photo", comment: "")

    case let .sticker(emoji, packName):
        let label = packName.isEmpty ? NSLocalizedString("notice.sticker", comment: "") : packName
        text = "\(emoji) \(label)"

    case .profile(let peer):
        text = String(
            format: NSLocalizedString("chats.profileLink", comment: ""), "@\(peer.suffix(6))")
    }
    return preview.forwarded
        ? String(format: NSLocalizedString("chats.forwarded", comment: ""), text)
        : text
}
