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
// Contracts mirror the native commands in main.rs, places.rs and chats.rs.
/**
 * @typedef {{
 *   locale_options: Command<[], [boolean, boolean]>,
 *   identity_state: Command<[], IdentityResponse>,
 *   storage_options: Command<[], SetupIdentity>,
 *   list_identities: Command<[], (ReadyIdentity | RecoveryIdentity)[]>,
 *   create_identity: Command<[{ mode: import('./identity-state.js').StorageMode }], ReadyIdentity>,
 *   select_identity: Command<[{ slot: string }], ReadyIdentity | RecoveryIdentity>,
 *   remove_identity: Command<[{ slot: string, peerId: string | null }], SetupIdentity | PickerIdentity>,
 *   quit: Command<[], void>,
 *   linked_device: Command<[], boolean>,
 *   set_tray: Command<[{ prefs: TrayPreferences | null, labels: [string, string, string, string] }], TrayState>,
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
 *   chat_rows: Command<[], import('./chat-state.js').ChatRow[]>,
 *   chat_history: Command<[{ peerId: string }], import('./chat-state.js').ChatHistory>,
 *   mark_chat_read: Command<[{ peerId: string }], number>,
 *   clear_chat: Command<[{ peerId: string }], number>,
 *   send_read_receipts: Command<[{ peerId: string }], number>,
 *   send_chat_text: Command<[{ peerId: string, text: string }], number[]>,
 *   peer_likes: Command<[{ peerId: string }], Likes>,
 *   like_peer: Command<[{ peerId: string, liked: boolean }], Likes>,
 *   block_peer: Command<[{ peerId: string }], BlockResult>,
 *   receive_mail: Command<[], number>
 * }} Commands
 * @typedef {<Name extends string>(command: Name,
 *   ...args: NoInfer<Name> extends keyof Commands ? Commands[NoInfer<Name>]['args'] : [args?: Record<string, unknown>]
 * ) => Promise<Name extends keyof Commands ? Commands[Name]['result'] : unknown>} Invoke
 * @typedef {<Result = unknown>(command: string, args?: Record<string, unknown>) => Promise<Result>} RawInvoke
 */
// Tauri injects this host API before page modules run; lib.dom does not declare it.
const bridge = /** @type {{ __TAURI__: { core: { invoke: RawInvoke } } }} */ (
  /** @type {unknown} */ (window)
);
export const invoke = /** @type {Invoke} */ (bridge.__TAURI__.core.invoke);
