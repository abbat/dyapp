# Desktop UX audit

This page audits the current Linux and Windows desktop UI (`clients/desktop/ui/`) against the
[v0.0.1 client screen draft](v0.0.1-client-screens.md), as input for the v0.0.1 desktop
redesign. It lists what exists, what the draft expects, and where they differ.

The control inventory below is generated from `index.html`, the `i18n/en.json` and
`i18n/ru.json` catalogues and `app.js`. Regenerate it with:

```sh
python3 -B tools/scripts/ui-inventory.py
```

The script is read-only and its output is stable, so a diff against this page shows UI drift.
Limits: it sees only controls written in `index.html`. Controls that `app.js` creates
(`#profile-scales`, `#search-scales`, the seed rows, result and identity lists) are not listed.
Visibility conditions are the matching `app.js` lines for the control's variable, listed as
written; loops and `querySelectorAll` groups are not attributed. The `hidden` column is the
default in `index.html`; "via" names the hidden ancestor.

## Control inventory

Source: clients/desktop/ui/index.html, app.js, i18n/en.json, i18n/ru.json.

### (main)

| id | element | i18n key | English | Russian | hidden | disabled |
|---|---|---|---|---|---|---|
| app-ready | p[role=status] | status.loading | Checking identity storage… | Проверяем хранилище ключа… | no | no |
| storage-error | p[role=alert] | - | - | - | yes | no |
| storage | fieldset | - | - | - | via #setup | no |
| system-choice | input[radio] | storage.system.choice | Use system credential storage (recommended) | Системное хранилище учётных данных (рекомендуется) | via #setup | no |
| system-reason | span[role=status] | storage.system.choice | Use system credential storage (recommended) | Системное хранилище учётных данных (рекомендуется) | via #setup | no |
| file-choice | input[radio] | storage.file.choice | Use an app data file | Файл в каталоге данных приложения | via #setup | no |
| create-identity | button | setup.create | Create identity | Создать идентичность | via #setup | no |
| add-identity | button | identity.add | Add a new identity | Добавить новую идентичность | via #picker | no |
| - | div[role=alert] | - | - | - | via #recovery | no |
| remove-identity | button | identity.remove | Remove this identity | Удалить эту идентичность | yes | no |
| switch-identity | button | identity.switch | Switch or add an identity | Сменить или добавить идентичность | yes | no |
| retry | button | recovery.retry | Retry | Повторить | via #recovery | no |
| menu-open | button | - | - | - | via #identity | no |
| nav-profile | button | - | - | - | via #identity | no |
| nav-search | button | - | - | - | via #identity | no |
| nav-network | button | - | - | - | via #identity | no |
| nav-settings | button | - | - | - | via #identity | no |
| remove-confirm | input[checkbox] | remove.confirm | I understand that this cannot be undone without a backup | Я понимаю, что без резервной копии это не отменить | via #remove | no |
| remove-cancel | button | remove.cancel | Cancel | Отмена | via #remove | no |
| remove-action | button | remove.action | Remove | Удалить | via #remove | yes |

Conditions in app.js:

- `storage-error` app.js:58: `storageError.hidden = true;`
- `storage-error` app.js:67: `storageError.hidden = false;`
- `system-choice` app.js:769: `systemChoice.disabled = state.systemAvailable !== 'true';`
- `file-choice` app.js:770: `fileChoice.disabled = false;`
- `create-identity` app.js:771: `createIdentity.disabled = false;`
- `remove-identity` app.js:56: `removeIdentity.hidden = !(section === sections[3]`
- `remove-identity` app.js:511: `removeIdentity.disabled = busy;`
- `switch-identity` app.js:54: `switchIdentity.hidden = section !== sections[3];`
- `switch-identity` app.js:64: `switchIdentity.hidden = true;`
- `switch-identity` app.js:510: `switchIdentity.disabled = busy;`
- `remove-action` app.js:808: `removeAction.disabled = true;`
- `remove-action` app.js:817: `removeConfirm.addEventListener('change', () => { removeAction.disabled = !removeConfirm.ch`
- `remove-action` app.js:819: `removeAction.disabled = true;`

### #profile-area

| id | element | i18n key | English | Russian | hidden | disabled |
|---|---|---|---|---|---|---|
| profile-area | section | - | - | - | via #identity | no |
| profile | form | - | - | - | via #identity | no |
| profile-country | select | profile.country | Country | Страна | via #identity | no |
| profile-place | input[text] | profile.place | Town or city | Населённый пункт | via #identity | no |
| birth-year | input[number] | profile.birthYear | Birth year | Год рождения | via #identity | no |
| birth-month | input[number] | profile.birthMonth | Birth month | Месяц рождения | via #identity | no |
| birth-day | input[number] | profile.birthDay | Birth day | День рождения | via #identity | no |
| profile-save | button | profile.save | Save draft | Сохранить черновик | via #identity | no |
| profile-preview-action | button | profile.previewAction | Preview public profile | Предпросмотр публичного профиля | via #identity | no |
| profile-preview | section | - | - | - | yes | no |
| profile-disclosure | input[checkbox] | profile.disclosure | I understand these public fields may remain available after deletion | Понимаю, что эти публичные данные могут остаться доступными после удаления | via #profile-preview | no |
| profile-attested | input[checkbox] | profile.attested | I confirm I am 18 or older for this publication | Подтверждаю, что мне не меньше 18 лет, для этой публикации | via #identity | no |
| profile-cancel | button | profile.cancel | Cancel puzzle work | Остановить поиск решения | yes | no |
| profile-publish | button | profile.publish | Publish profile | Опубликовать профиль | via #identity | no |
| profile-status | p[role=status] | - | - | - | via #identity | no |
| profile-search-wait | p[role=status] | - | - | - | yes | no |

Conditions in app.js:

- `profile-preview` app.js:464: `profilePreview.hidden = true;`
- `profile-preview` app.js:497: `profilePreview.hidden = false;`
- `profile-preview` app.js:528: `profilePreview.hidden = true;`
- `profile-cancel` app.js:515: `profileCancel.hidden = !solving;`
- `profile-cancel` app.js:552: `profileCancel.disabled = true;`
- `profile-cancel` app.js:568: `profileCancel.hidden = false;`
- `profile-cancel` app.js:571: `profileCancel.hidden = true;`
- `profile-cancel` app.js:583: `profileCancel.disabled = false;`
- `profile-search-wait` app.js:408: `searchWait.hidden = minutes == null;`

### #search-area

| id | element | i18n key | English | Russian | hidden | disabled |
|---|---|---|---|---|---|---|
| search-area | section | - | - | - | yes | no |
| search | form | - | - | - | via #search-area | no |
| search-employer | select | search.employer | Employer | Работодатель | via #search-area | no |
| - | option | search.employer.any | Any employer | Любой работодатель | via #search-area | no |
| - | option | search.employer.same | Same company as typed below | Та же компания, что указана ниже | via #search-area | no |
| - | option | search.employer.exclude | Hide companies typed below | Скрыть компании, указанные ниже | via #search-area | no |
| search-company | input[text] | search.company | Company names (separate several with ;) | Названия компаний (несколько через ;) | via #search-area | no |
| search-age-min | input[number] | search.ageMin | Age from | Возраст от | via #search-age | no |
| search-age-max | input[number] | search.ageMax | Age to | Возраст до | via #search-age | no |
| search-country | input[text] | search.country | Country code (two letters) | Код страны (две буквы) | via #search-country-row | no |
| search-place | input[text] | profile.place | Town or city | Населённый пункт | via #search-place-row | no |
| search-retry | button | search.retry | Retry | Повторить | yes | no |
| search-again | button | search.again | Load another sample | Загрузить другую выборку | yes | no |
| run-search | button | search.run | Search | Искать | via #search-area | no |
| search-status | p[role=status] | - | - | - | via #search-area | no |

Conditions in app.js:

- `search-retry` app.js:690: `searchRetry.hidden = true;`
- `search-retry` app.js:703: `searchRetry.hidden = !result.partial;`
- `search-retry` app.js:711: `searchRetry.hidden = false;`
- `search-retry` app.js:722: `searchRetry.hidden = true;`
- `search-again` app.js:691: `searchAgain.hidden = true;`
- `search-again` app.js:704: `searchAgain.hidden = false;`
- `search-again` app.js:723: `searchAgain.hidden = true;`

### #network-area

| id | element | i18n key | English | Russian | hidden | disabled |
|---|---|---|---|---|---|---|
| network-area | section | - | - | - | yes | no |
| network | form | - | - | - | via #network-area | no |
| seeds | textarea | network.seeds | Bootstrap nodes, one host:port per line | Узлы начальной загрузки, по одному host:port в строке | via #network-area | no |
| network-name | input[text] | network.name | Network name (blank = public) | Имя сети (пусто — публичная) | via #network-area | no |
| network-status | p[role=status] | - | - | - | via #network-area | no |
| cancel-check | button | network.cancel | Cancel | Отмена | yes | no |
| check-connection | button | network.check | Check connection | Проверить подключение | via #network-area | no |
| continue-connection | button | network.continue | Continue | Продолжить | via #network-area | yes |

Conditions in app.js:

- `cancel-check` app.js:242: `cancelButton.hidden = false;`
- `cancel-check` app.js:259: `cancelButton.hidden = true;`
- `check-connection` app.js:240: `checkButton.disabled = true;`
- `check-connection` app.js:258: `checkButton.disabled = false;`
- `continue-connection` app.js:208: `continueButton.disabled = true;`
- `continue-connection` app.js:241: `continueButton.disabled = true;`
- `continue-connection` app.js:248: `continueButton.disabled = !result.some((row) => row.status === 'connected');`
- `continue-connection` app.js:268: `network.addEventListener('input', () => { continueButton.disabled = true; });`
- `continue-connection` app.js:273: `continueButton.disabled = true;`

### #settings

| id | element | i18n key | English | Russian | hidden | disabled |
|---|---|---|---|---|---|---|
| settings | section | - | - | - | yes | no |
| settings-error | p[role=alert] | - | - | - | yes | no |
| settings-remove | button | identity.remove | Remove this identity | Удалить эту идентичность | via #settings | no |
| settings-switch | button | identity.switch | Switch or add an identity | Сменить или добавить идентичность | via #settings | no |
| settings-create | button | identity.add | Add a new identity | Добавить новую идентичность | via #settings | no |
| node | form | - | - | - | yes | no |
| node-enabled | input[checkbox] | node.enabled | Help the network while the app is open: store and search other people's profiles and messages | Помогать сети, пока приложение открыто: хранить и искать чужие профили и сообщения | via #node | no |
| node-disk | input[number] | node.disk | Disk limit, MB | Лимит диска, МБ | via #node | no |
| node-connections | input[number] | node.connections | Connection limit | Лимит соединений | via #node | no |
| node-status | p[role=status] | - | - | - | via #node | no |
| node-save | button | node.save | Save node settings | Сохранить настройки узла | via #node | no |

Conditions in app.js:

- `settings-error` app.js:148: `error.hidden = true;`
- `settings-error` app.js:153: `error.hidden = true;`
- `settings-error` app.js:157: `error.hidden = false;`
- `node` app.js:300: `nodeForm.hidden = true;`
- `node` app.js:303: `nodeForm.hidden = false;`
- `node-save` app.js:325: `nodeSave.disabled = true;`
- `node-save` app.js:330: `.finally(() => { nodeSave.disabled = false; });`

## Mismatches with the screen draft

Checked 2026-10-28 against the inventory above and `app.js`; the shell items were rechecked on
2026-10-29, after the Profile, Search, Network and Settings areas replaced the one-page Ready view.

### Controls on the wrong screen

- The Network area uses `Check` and `Continue`; the draft's Settings card has `Check and apply`.
  The control ids differ (`continue-connection` against the draft's `continue-profile` and
  `apply-network`).
- `#node` (`node-enabled`, `node-disk`, `node-connections`, `node-save`) lives in Settings but
  appears in no screen card or first-release settings list.
- `settings-create`, `settings-switch` and `add-identity` (picker) all add an identity, and
  `switch-identity` and `remove-identity` on the recovery screen repeat `settings-switch` and
  `settings-remove` outside Settings. The draft puts them in Settings only.

### Missing screens and states

- Settings lacks the language select, the update-check interval, last check, `Check now`, the
  update notice and badge, the tray and close-to-tray toggles, the storage directory and usage
  lines, and the status bar. These are in the first-release settings inventory.
- No `Copy ID` control next to the Peer ID (`peer-id` is plain text).
- No `retry-connection`: a failed check is retried with `check-connection`. There is no
  explicit `save-draft` or `apply-network`; edits save on change.
- Profile: no `More optional fields`, no About, no photos (all planned or blocked in the
  draft). The draft's birth-date precision select is absent because three number inputs
  replaced it (the integration contract describes the inputs, the draft control list does not).
- Search: no control to open a result for the verified public profile, no compatibility
  counts (planned), no `More filters` button (filters appear after the first sample).
- Picker, recovery and remove screens have no control list in the draft, only prose;
  [Desktop screens](desktop-screens.md) now lists their IDs.
- Published and retry states of the profile are text in `profile-status`; there is no
  distinct published view.

### Controls without a user task in the draft

- `profile-disclosure` and `profile-attested` are two confirmations in prose, not in the card.
- `node-*` (see above) and `profile-search-wait` (a wait notice) have no task text.
- `system-reason` and `network-status` serve the draft's status lines but have no defined
  states beyond the prose.

### Tasks in the draft without a control

- Copy Peer ID; Show icon in tray; Close window to tray; Language; Update check interval and
  Check now; storage directory and usage; compatibility counts; open a result; More filters;
  Gender and relationship goal as named fields (only through the generated scale selects,
  which the inventory cannot see); Remove selected identity from the identity list (the list
  items are generated).

## Screenshots

`make ui-test-linux` saves every state the Linux UI test visits, in English and Russian at 440
and 1280 px wide, as `build/ux-research/current/<state>-<lang>-<width>.png` (not committed).
The shipped shell is reviewed on these files and the prototype on its own
([desktop heuristic review](desktop-heuristics.md)); automated accessibility checks are planned.
