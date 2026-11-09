export const identityState = { current: null, generation: 0 };
/** @param {number} generation */
export const identityCurrent = (generation) => generation === identityState.generation;
