import { enhanceChoice } from '../ui/profile-choice.js';
import { showState } from './keyboard.js';
enhanceChoice(
  document.querySelector('select[data-choice-picker]'),
  document.querySelector('label[for="profile-goal"]'),
  window.strings['action.cancel'],
  () => 0,
);
const dialog = document.getElementById('profile-goal-picker');
if (document.documentElement.dataset.screen === 'profile-goal-dialog') {
  dialog.dataset.returnState = 'profile';
  dialog.addEventListener('close', () => {
    if (document.documentElement.dataset.screen === 'profile-goal-dialog') showState('profile');
  });
  document.getElementById('profile-goal-choice').click();
}
