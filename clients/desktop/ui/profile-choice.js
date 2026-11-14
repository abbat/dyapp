/** @param {HTMLSelectElement} select @param {HTMLLabelElement} label @param {string} cancelText */
const choiceShell = (select, label, cancelText) => {
  const opener = document.createElement('button');
  opener.type = 'button';
  opener.id = `${select.id}-choice`;
  opener.className = 'btn field-choice';
  label.id ||= `${select.id}-label`;
  label.htmlFor = opener.id;
  const value = document.createElement('span');
  value.id = `${select.id}-value`;
  opener.append(value);
  opener.setAttribute('aria-labelledby', `${label.id} ${value.id}`);
  opener.setAttribute('aria-haspopup', 'dialog');
  const dialog = document.createElement('dialog');
  dialog.id = `${select.id}-picker`;
  dialog.className = 'modal';
  dialog.setAttribute('role', 'dialog');
  dialog.dataset.dialogKind = 'choice';
  dialog.dataset.opener = opener.id;
  opener.setAttribute('aria-controls', dialog.id);
  const title = document.createElement('h2');
  title.id = `${dialog.id}-title`;
  title.textContent = label.textContent;
  dialog.setAttribute('aria-labelledby', title.id);
  const list = document.createElement('div');
  list.className = 'choice-options';
  const cancel = document.createElement('button');
  cancel.type = 'button';
  cancel.id = `${dialog.id}-cancel`;
  cancel.className = 'btn';
  cancel.textContent = cancelText;
  cancel.dataset.i18n = 'action.cancel';
  cancel.addEventListener('click', () => dialog.close());
  const actions = document.createElement('div');
  actions.className = 'actions';
  actions.append(cancel);
  dialog.append(title, list, actions);
  return { opener, value, dialog, list, cancel };
};
/**
 * @param {HTMLSelectElement} select @param {HTMLDivElement} list
 * @param {HTMLDialogElement} dialog @param {(option: HTMLOptionElement) => void} apply
 */
const choiceButtons = (select, list, dialog, apply) => {
  return [...select.options].map((option) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'btn';
    button.textContent = option.textContent;
    button.disabled = option.disabled;
    button.addEventListener('click', () => {
      if (!dialog.open) return;
      apply(option);
      dialog.close();
    });
    list.append(button);
    return { option, button };
  });
};
/**
 * Keep the core-generated select as the form value, with a wrapping choice dialog.
 * @param {HTMLSelectElement} select
 * @param {HTMLLabelElement} label
 * @param {string} cancelText
 * @param {() => number} currentOwner
 */
export const enhanceChoice = (select, label, cancelText, currentOwner) => {
  const row = select.parentElement;
  if (!row) throw new Error('Choice field must be attached to its row');
  const { opener, value, dialog, list, cancel } = choiceShell(select, label, cancelText);
  /** @type {number | null} */
  let openedOwner = null;
  const sync = () => {
    value.textContent = select.selectedOptions[0]?.textContent || '';
    opener.disabled = select.matches(':disabled');
  };
  const choices = choiceButtons(select, list, dialog, (option) => {
    if (
      select.isConnected &&
      currentOwner() === openedOwner &&
      !select.matches(':disabled') &&
      !option.disabled
    ) {
      select.value = option.value;
      sync();
      select.dispatchEvent(new Event('change', { bubbles: true }));
    }
  });
  dialog.addEventListener('close', () => {
    if (opener.isConnected && currentOwner() === openedOwner) opener.focus({ preventScroll: true });
  });
  opener.addEventListener('click', () => {
    if (dialog.open || !select.isConnected || select.matches(':disabled')) return;
    openedOwner = currentOwner();
    sync();
    for (const choice of choices)
      choice.button.setAttribute('aria-pressed', String(choice.option.selected));
    dialog.showModal();
    (choices.find((choice) => choice.option.selected)?.button || cancel).focus();
  });
  select.addEventListener('change', sync);
  new MutationObserver(sync).observe(select, { attributes: true, attributeFilter: ['disabled'] });
  // scaleRow classification uses the final select ID; retain that existing form contract.
  select.before(opener, dialog);
  select.hidden = true;
  sync();
};
