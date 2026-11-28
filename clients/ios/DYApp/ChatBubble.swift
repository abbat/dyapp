import SwiftUI

struct ChatBubble: View {
    let message: ChatMessage
    let own: Bool
    let writable: Bool
    let sending: Bool
    let openProfile: (String) -> Void
    let edit: () -> Void
    let delete: () -> Void
    let forward: () -> Void

    var body: some View {
        HStack {
            if own { Spacer(minLength: 48) }
            if own, !message.deleted { menu }
            VStack(alignment: .leading, spacing: 4) {
                if let shared = sharedProfile(message) {
                    // It names only the short Peer ID: no message fetches anything before the press.
                    Button {
                        openProfile(shared)
                    } label: {
                        Label(chatPreview(message), systemImage: "person.crop.circle")
                    }
                    .accessibilityIdentifier("msg-profile")
                } else {
                    Text(verbatim: chatPreview(message)).textSelection(.enabled)
                        .accessibilityIdentifier("msg-text")
                    if !message.deleted, let text = message.text {
                        let hosts = chatLinkHosts(text).map { host in
                            host.unicode.map { "\(host.ascii) (\($0))" } ?? host.ascii
                        }
                        if !hosts.isEmpty {
                            Text(verbatim: hosts.joined(separator: ", "))
                                .font(.caption)
                                .textSelection(.enabled)
                                .accessibilityIdentifier("chat-link-hosts")
                        }
                    }
                }
                let meta = chatMeta(message)
                if !meta.isEmpty { Text(verbatim: meta).font(.caption) }
            }
            .padding(8)
            .background(
                RoundedRectangle(cornerRadius: 12).fill(
                    own ? Color.accentColor.opacity(0.2) : Color.secondary.opacity(0.15)
                ))
            if !own, !message.deleted { menu }
            if !own { Spacer(minLength: 48) }
        }
    }

    // A button rather than a context menu: right click on selectable text opens the text's own menu.
    private var menu: some View {
        Menu {
            Button("chat.forward", action: forward).disabled(sending)
                .accessibilityIdentifier("msg-forward")
            if own, writable {
                Button("chat.edit", action: edit).disabled(sending).accessibilityIdentifier(
                    "msg-edit")
            }
            if own {
                Button("chat.delete", role: .destructive, action: delete).disabled(sending)
                    .accessibilityIdentifier("msg-delete")
            }
        } label: {
            Image(systemName: "ellipsis")
                #if os(iOS)
                    // The bare glyph is 18 by 5 points, far below a touch target.
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
                #endif
        }
        .fixedSize()
        .accessibilityLabel(Text("msg.menu"))
        .accessibilityIdentifier(own ? "msg-menu" : "msg-received-menu")
    }
}
