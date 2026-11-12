/**
 * @template {unknown[]} Args
 * @template Result
 * @typedef {{ args: Args, result: Result }} Command
 */
/**
 * @typedef {import('./identity-state.js').ReadyIdentity} ReadyIdentity
 * @typedef {import('./identity-state.js').RecoveryIdentity} RecoveryIdentity
 * @typedef {import('./identity-state.js').SetupIdentity} SetupIdentity
 * @typedef {import('./identity-state.js').PickerIdentity} PickerIdentity
 * @typedef {import('./identity-state.js').IdentityResponse} IdentityResponse
 * @typedef {import('./profile-fields.js').PickerScale} PickerScale
 * @typedef {{ id: number, name: string, region: string }} Place
 * @typedef {{ show: boolean, closeToTray: boolean }} TrayPreferences
 * @typedef {TrayPreferences & { available: boolean }} TrayState
 * @typedef {{ ours: boolean, matched: boolean, passed: boolean }} Likes
 * @typedef {{ revocation: 'notNeeded' | 'pending' | 'confirmed', detail: string | null }} BlockResult
 */
/**
 * @typedef {ArrayBuffer | number[]} BinaryResponse
 * @typedef {{ seeds: string[], name: string }} NetworkDraft
 * @typedef {{ status: 'connected', peerId: string, reason?: never } |
 *   { status: 'unreachable', reason: string, peerId?: never } |
 *   { status: 'invalidAddress' | 'invalidPeerId' | 'duplicate' | 'wrongPeer' | 'wrongNetwork',
 *     peerId?: never, reason?: never }} NetworkCheckRow
 * @typedef {{ enabled: boolean, diskMb: number, maxConnections: number }} NodeSettings
 * @typedef {{ status: 'running', peerId: string } |
 *   { status: 'off' | 'noNetwork', peerId?: never }} NodeStatus
 * @typedef {{ identityDir: string | null, messagesPath: string, messages: number | null,
 *   nodePath: string | null, node: number | null, nodeLimit: number | null,
 *   free: number | null }} StorageUsage
 * @typedef {{ traffic: [number, number] | null, messages: number | null }} StatusBar
 * @typedef {{ sendViews: boolean, sendReadReceipts: boolean, sendTyping: boolean,
 *   presenceMatches: boolean, notifications: boolean,
 *   noticeContent: 'fixed' | 'name' | 'text' }} Privacy
 * @typedef {{ peerId: string, name: string, verified: boolean | null, photo: number[] }} PeerName
 * @typedef {{ size: number, modules: boolean[] }} QrMatrix
 * @typedef {'multiple' | 'unreadable' | 'invalid' | 'matched' | 'foreign' | 'notFound'} QrScan
 * @typedef {{ code: string, qr: QrMatrix }} LinkCode
 * @typedef {{ key: number[], this: boolean }} Device
 * @typedef {'any' | 'same' | 'exclude'} EmployerChoice
 * @typedef {Partial<{ offered: number[], picked: Record<string, number>, country: string,
 *   interest: string, place: number | null, ageMin: string, ageMax: string, heightMin: string,
 *   heightMax: string, employer: EmployerChoice, company: string, limit: number,
 *   mutual: boolean }>} SearchQuery
 * @typedef {{ age: boolean, height: boolean, interest: boolean, country: boolean,
 *   place: boolean, mutual: boolean }} SearchFilters
 * @typedef {{ kind: 'noNodes' | 'unavailable' | 'timeout' | 'puzzle' | 'invalid' } |
 *   { kind: 'rateLimited', retryAfterMs: number }} SearchFailure
 * @typedef {{ peerId: string, draft: import('./profile-fields.js').ProfileDraft,
 *   fullPhotos: number[][], version: number, age?: [number, number] | null }} FoundProfile
 * @typedef {{ offered: number[], filters: SearchFilters, profiles: FoundProfile[],
 *   partial: boolean, failures: SearchFailure[] }} SearchResults
 * @typedef {'byNetwork' | 'always' | 'onTap'} AutoDownloadMode
 * @typedef {{ enabled: boolean, limitBytes: number, usedBytes: number }} MediaCache
 * @typedef {{ id: number[], bytes: number, name: string | null, stickers: number | null,
 *   error: string | null }} StickerPack
 * @typedef {{ entries: StickerPack[], next: number[] | null, usedBytes: number,
 *   unrecognizedFiles: number }} StickerPacks
 * @typedef {{ width: number, height: number, pixels: number[] }} StickerFrame
 * @typedef {{ index: number, emoji: string, frame: StickerFrame }} PickerSticker
 * @typedef {{ packId: number[], name: string, total: number, entries: PickerSticker[],
 *   next: number | null }} SavedStickerPage
 * @typedef {{ entries: (PickerSticker & { packId: number[], packName: string })[],
 *   next: number | null }} RecentStickerPage
 * @typedef {{ peerId: string, groupId: number[], author: string, id: number[] }} ChatStickerSelection
 * @typedef {{ peerId: string, packId: number[], index: number }} SavedStickerSelection
 * @typedef {{ state: 'saved' | 'new' | 'failed' | 'waiting' | 'downloading',
 *   generation: number[] | null, failure: 'fetch' | 'invalid' | 'quota' | 'storage' | 'stickerMissing' | null,
 *   retryAtMs: number | null }} StickerPackStatus
 * @typedef {{ kind: 'request' | 'retry' | 'fresh' | 'stopWaiting',
 *   generation?: number[] | null }} StickerPackAction
 */
// Contracts mirror the shared desktop command wrappers and their serialized DTOs.
/**
 * @typedef {{
 *   locale_options: Command<[], [boolean, boolean]>,
 *   identity_state: Command<[], IdentityResponse>,
 *   storage_options: Command<[], SetupIdentity>,
 *   list_identities: Command<[], (ReadyIdentity | RecoveryIdentity)[]>,
 *   create_identity: Command<[{ mode: import('./identity-state.js').StorageMode }], ReadyIdentity>,
 *   select_identity: Command<[{ slot: string }], ReadyIdentity | RecoveryIdentity>,
 *   remove_identity: Command<[{ slot: string, peerId?: string | null }], SetupIdentity | PickerIdentity>,
 *   quit: Command<[], void>,
 *   linked_device: Command<[], boolean>,
 *   set_tray: Command<[{ prefs?: TrayPreferences | null, labels: [string, string, string, string] }], TrayState>,
 *   profile_scales: Command<[], PickerScale[]>,
 *   load_profile: Command<[], import('./profile-fields.js').ProfileView>,
 *   save_profile: Command<[{ draft: import('./profile-fields.js').ProfileDraftInput }], import('./profile-fields.js').ProfileView>,
 *   solve_profile: Command<[{ attested: boolean }], void>,
 *   cancel_profile_solve: Command<[], void>,
 *   publish_profile: Command<[{ attested: boolean }], import('./profile-fields.js').ProfileView>,
 *   hide_profile: Command<[], import('./profile-fields.js').ProfileView>,
 *   withdraw_profile: Command<[], import('./profile-fields.js').ProfileView>,
 *   send_search_profile: Command<[], import('./profile-fields.js').ProfileView>,
 *   place_countries: Command<[], string[]>,
 *   place_name: Command<[{ country: string, id: number, lang: string }], Place | null>,
 *   search_places: Command<[{ country: string, prefix: string, lang: string, limit: number }], Place[]>,
 *   chat_composer: Command<[{ state: import('./chat-state.js').ConversationState | null, text: string }], { tooLong: boolean, blank: boolean, sendable: boolean }>,
 *   chat_rows: Command<[], import('./chat-state.js').ChatRow[]>,
 *   chat_history: Command<[{ peerId: string }], import('./chat-state.js').ChatHistory>,
 *   mark_chat_read: Command<[{ peerId: string }], number>,
 *   clear_chat: Command<[{ peerId: string }], number>,
 *   send_read_receipts: Command<[{ peerId: string }], number>,
 *   send_chat_text: Command<[{ peerId: string, text: string }], number[]>,
 *   peer_likes: Command<[{ peerId: string }], Likes>,
 *   like_peer: Command<[{ peerId: string, liked: boolean }], Likes>,
 *   block_peer: Command<[{ peerId: string }], BlockResult>,
 *   receive_mail: Command<[], number>,
 *   load_network: Command<[{ file: 'draft' | 'active' }], NetworkDraft | null>,
 *   save_network_draft: Command<[{ draft: NetworkDraft }], void>,
 *   check_connection: Command<[{ draft: NetworkDraft, waitMs: number }], NetworkCheckRow[]>,
 *   cancel_connection_check: Command<[], void>,
 *   connect_network: Command<[{ waitMs: number }], boolean>,
 *   activate_connection: Command<[], NetworkDraft>,
 *   load_node_settings: Command<[], NodeSettings>,
 *   save_node_settings: Command<[{ settings: Partial<NodeSettings> }], NodeStatus>,
 *   apply_node: Command<[], NodeStatus>,
 *   node_status: Command<[], NodeStatus>,
 *   storage_usage: Command<[], StorageUsage>,
 *   status: Command<[], StatusBar>,
 *   privacy_settings: Command<[], Privacy>,
 *   set_privacy: Command<[{ privacy: Privacy }], void>,
 *   blocked_peers: Command<[], string[]>,
 *   unblock_peer: Command<[{ peerId: string }], void>,
 *   set_muted: Command<[{ peerId: string, muted: boolean }], void>,
 *   passed_peers: Command<[], string[]>,
 *   pass_peer: Command<[{ peerId: string, passed: boolean }], Likes>,
 *   contact_note: Command<[{ peerId: string }], string>,
 *   set_contact_note: Command<[{ peerId: string, note: string }], void>,
 *   favorite_peers: Command<[], string[]>,
 *   favorite_state: Command<[{ peerId: string }], boolean>,
 *   set_favorite: Command<[{ peerId: string, favorite: boolean }], void>,
 *   contact_names: Command<[{ peerIds: string[] }], PeerName[]>,
 *   set_contact_name: Command<[{ peerId: string, name: string }], void>,
 *   contact_safety_qr: Command<[{ peerId: string }], QrMatrix>,
 *   scan_contact_qr: Command<[{ peerId: string, width: number, height: number, pixels: number[] }], QrScan>,
 *   contact_safety_number: Command<[{ peerId: string }], string>,
 *   confirm_contact: Command<[{ peerId: string }], FoundProfile>,
 *   contact_verified: Command<[{ peerId: string }], boolean>,
 *   contact_new_device: Command<[{ peerId: string }], boolean>,
 *   acknowledge_new_device: Command<[{ peerId: string }], void>,
 *   unverify_contact: Command<[{ peerId: string }], void>,
 *   copy_safety_number: Command<[{ peerId: string }], string>,
 *   clear_safety_clipboard: Command<[{ expected: string }], boolean>,
 *   start_link: Command<[], LinkCode>,
 *   cancel_link: Command<[], void>,
 *   link_pending: Command<[], boolean>,
 *   poll_link: Command<[], ReadyIdentity | null>,
 *   check_link_code: Command<[{ code: string }], void>,
 *   send_link: Command<[{ code: string }], void>,
 *   list_devices: Command<[{ live: boolean }], Device[]>,
 *   revoke_device: Command<[{ deviceKey: number[] }], import('./profile-fields.js').ProfileView>,
 *   search_profiles: Command<[{ query: SearchQuery }], SearchResults>,
 *   liked_you: Command<[], FoundProfile[]>,
 *   shared_profile: Command<[{ peerId: string }], FoundProfile>,
 *   share_profile: Command<[{ peerId: string, targetPeerId: string }], number[]>,
 *   edit_chat_text: Command<[{ peerId: string, groupId: number[], messageId: number[], text: string }], number[]>,
 *   delete_chat_text: Command<[{ peerId: string, groupId: number[], messageId: number[] }], number[]>,
 *   forward_chat_text: Command<[{ sourcePeerId: string, sourceGroupId: number[], sourceAuthor: string, sourceMessageId: number[], targetPeerId: string }], { id: number[], queued: string | null }>,
 *   announce_presence: Command<[], boolean>,
 *   photo_picked_byte_limit: Command<[], number>,
 *   photo_decode_plan: Command<[{ mime: string, width: number, height: number, bytes: number, purpose: 'profile' | 'chat' }], { width: number, height: number }>,
 *   send_chat_photo: Command<[Uint8Array, { headers: PhotoHeaders & { 'dyapp-peer-id': string } }], number[]>,
 *   add_profile_photo: Command<[Uint8Array, { headers: PhotoHeaders }], import('./profile-fields.js').ProfileView>,
 *   remove_profile_photo: Command<[{ index: number, revision: number[] }], import('./profile-fields.js').ProfileView>,
 *   move_profile_photo: Command<[{ from: number, to: number, revision: number[] }], import('./profile-fields.js').ProfileView>,
 *   profile_photo: Command<[{ hash: number[] }], BinaryResponse>,
 *   fetch_peer_photos: Command<[{ hashes: number[][], full: boolean }], void>,
 *   peer_photo: Command<[{ hash: number[] }], BinaryResponse>,
 *   peer_photo_auto: Command<[], boolean>,
 *   chat_photo_original: Command<[{ peerId: string, author: string, id: number[] }], BinaryResponse>,
 *   chat_photo_auto: Command<[{ peerId: string }], boolean>,
 *   auto_download_mode: Command<[], AutoDownloadMode>,
 *   set_auto_download_mode: Command<[{ mode: AutoDownloadMode }], void>,
 *   media_cache: Command<[{ identityId: string }], MediaCache>,
 *   set_media_cache: Command<[{ identityId: string, enabled: boolean, limitBytes: number }], MediaCache>,
 *   clear_media_cache: Command<[{ identityId: string }], MediaCache>,
 *   remove_saved_sticker_pack: Command<[{ identityId: string, packId: number[] }], boolean>,
 *   import_sticker_pack: Command<[{ identityId: string, bytes: number[] }], number[]>,
 *   saved_sticker_packs: Command<[{ identityId: string, after?: number[] | null }], StickerPacks>,
 *   saved_sticker_page: Command<[{ identityId: string, packId: number[], after: number }], SavedStickerPage>,
 *   recent_sticker_page: Command<[{ identityId: string, after: number }], RecentStickerPage>,
 *   chat_sticker_frame: Command<[{ identityId: string, selection: ChatStickerSelection, manual: boolean }], BinaryResponse>,
 *   chat_sticker_pack_status: Command<[{ identityId: string, selection: ChatStickerSelection }], StickerPackStatus>,
 *   change_sticker_pack_request: Command<[{ identityId: string, selection: ChatStickerSelection, action: StickerPackAction }], StickerPackStatus>,
 *   send_saved_sticker: Command<[{ identityId: string, selection: SavedStickerSelection }], number[]>
 * }} Commands
 * @typedef {<Name extends keyof Commands>(command: Name,
 *   ...args: Commands[NoInfer<Name>]['args']
 * ) => Promise<Commands[Name]['result']>} Invoke
 * @typedef {{ 'dyapp-width': string, 'dyapp-height': string }} PhotoHeaders
 * @typedef {<Result = unknown>(command: string, args?: Record<string, unknown> | Uint8Array,
 *   options?: { headers?: Record<string, string> }) => Promise<Result>} RawInvoke
 */
// Tauri injects this host API before page modules run; lib.dom does not declare it.
const bridge = /** @type {{ __TAURI__: { core: { invoke: RawInvoke } } }} */ (
  /** @type {unknown} */ (window)
);
export const invoke = /** @type {Invoke} */ (bridge.__TAURI__.core.invoke);
