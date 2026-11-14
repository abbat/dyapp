/**
 * @typedef {'none' | 'requestSent' | 'requestReceived' | 'open' | 'closed'} ConversationState
 * @typedef {'pending' | 'nodeAccepted' | 'deviceReceived'} Delivery
 * @typedef {{ mime: string, width: number, height: number, size: number,
 *   hoursLeft: number | null, kept: boolean, preview: number[] }} ChatPhoto
 * @typedef {{ packId: number[], sha256: number[], emoji: string, packName: string }} ChatSticker
 * @typedef {{ groupId: number[], author: string, id: number[], text: string | null,
 *   linkHosts: string[], edited: boolean, deleted: boolean, unread: boolean, forwarded: boolean,
 *   delivery: Delivery | null, read: boolean, sentAtMs: number | null,
 *   photo: ChatPhoto | null, sticker: ChatSticker | null, sharedPeer: string | null }} ChatMessage
 * @typedef {{ state: ConversationState, messages: ChatMessage[], activeRecently: boolean,
 *   muted: boolean }} ChatHistory
 * @typedef {{ kind: 'empty' | 'deleted' | 'photo' } |
 *   { kind: 'text', text: string } | { kind: 'sticker', emoji: string, packName: string } |
 *   { kind: 'profile', peerId: string }} ChatPreviewBody
 * @typedef {{ body: ChatPreviewBody, forwarded: boolean, sentAtMs: number | null }} ChatListPreview
 * @typedef {{ peerId: string, state: ConversationState, lastMessage: ChatMessage | null,
 *   preview: ChatListPreview, unreadCount: number, activeRecently: boolean, muted: boolean }} ChatRow
 * @typedef {{ sourcePeerId: string, sourceGroupId: number[], sourceAuthor: string,
 *   sourceMessageId: number[], text: string, generation: number, request: number,
 *   share?: never }} ForwardMessage
 * @typedef {{ share: string, generation: number }} ShareProfile
 */
/** @type {{ activeChat: (ChatHistory & { peerId: string }) | null,
 *   forwardTarget: ForwardMessage | ShareProfile | null, forwardBusy: boolean }} */
export const chatState = { activeChat: null, forwardTarget: null, forwardBusy: false };
