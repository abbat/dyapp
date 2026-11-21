import * as controller from '../prototype/keyboard.js';
import { beginFixture, endFixture } from './prototype-keyboard.js';

// Exceed WebKit's rapid-history limit without pacing or disabling the real state controller.
export const checkTransientNavigation = async () => {
  const initial = document.documentElement.dataset.screen;
  const url = location.href;
  const focused = document.activeElement;
  const failure = new Error('Controlled calibration rejection');
  try {
    try {
      await beginFixture('navigation-history');
      try {
        for (let step = 0; step < 128; step += 1) {
          const state = step % 2 ? 'chat' : 'profile';
          controller.showState(state);
          if (document.documentElement.dataset.screen !== state || location.href !== url)
            throw new Error('Transient navigation changed the URL or lost its presentation state');
        }
        throw failure;
      } finally {
        endFixture();
      }
    } catch (error) {
      if (error !== failure) throw error;
    }
    const next = initial === 'profile' ? 'chat' : 'profile';
    controller.showState(next);
    if (location.hash !== `#${next}`)
      throw new Error('Rejected calibration left ordinary URL navigation suspended');
  } finally {
    controller.showState(initial);
    if (focused?.isConnected) focused.focus();
  }
};
