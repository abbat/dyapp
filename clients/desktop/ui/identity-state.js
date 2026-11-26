/**
 * @typedef {'file' | 'system'} StorageMode
 * @typedef {{ status: 'ready', peerId: string, mode: StorageMode, slot: string,
 *   keyPath?: string, storageStatus?: 'unconfirmed', reason?: never }} ReadyIdentity
 * @typedef {{ status: 'recovery', reason: string, slot: string, mode?: StorageMode,
 *   peerId?: string, storageStatus?: 'unconfirmed', keyPath?: never }} RecoveryIdentity
 * @typedef {{ status: 'setup', systemAvailable: 'true' | 'false', systemReason?: string,
 *   storageStatus?: 'unconfirmed' }} SetupIdentity
 * @typedef {{ status: 'picker', storageStatus?: 'unconfirmed' }} PickerIdentity
 * @typedef {ReadyIdentity | RecoveryIdentity | SetupIdentity | PickerIdentity} IdentityResponse
 */
/** @type {{ current: ReadyIdentity | RecoveryIdentity | null, generation: number }} */
export const identityState = { current: null, generation: 0 };
/** @param {number} generation */
export const identityCurrent = (generation) => generation === identityState.generation;
