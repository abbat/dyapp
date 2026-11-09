import { identityState, identityCurrent } from './identity-state.js';
import { strings } from './locale.js';
export const controller = {};

controller.clipboardCopyToken = 0;
const copyFrom = (button, source, text = () => source.textContent) =>
  button.addEventListener('click', () => {
    controller.clipboardCopyToken += 1;
    const generation = identityState.generation;
    const value = text();
    controller.copied = Promise.resolve(value)
      .then((copy) => (identityCurrent(generation) ? navigator.clipboard.writeText(copy) : null))
      .then(
        () => {
          if (!identityCurrent(generation)) return;
          button.textContent = strings['action.copied'];
          setTimeout(() => {
            if (identityCurrent(generation)) button.textContent = strings['action.copy'];
          }, 2000);
        },
        () => {
          if (!identityCurrent(generation)) return;
          source.textContent = value;
          window.getSelection().selectAllChildren(source);
        },
      );
  });

export { copyFrom };
