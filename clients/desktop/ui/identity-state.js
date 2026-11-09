export const identityState = { current: null, generation: 0 };
export const identityCurrent = (generation) => generation === identityState.generation;
