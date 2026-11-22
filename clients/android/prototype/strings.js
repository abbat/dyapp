// Android wording over the desktop prototype strings (loaded first); the shipped strings live in
// app/src/main/res. Only keys that differ on Android or exist only here.
Object.assign(STRINGS.en, {
  'owner.intro': 'On the new device choose Receive a profile, then paste its code here.',
  'owner.identity': 'Identity giving full control',
  'owner.preparing': 'Preparing confirmation. No private data has been sent.',
  'owner.prepareError': 'Could not prepare confirmation. No private data has been sent.',
  'owner.sending': 'Sending encrypted profile data. Wait for confirmation.',
  'devices.this': 'This device',
  'devices.revoking': 'Disconnecting this device. Wait for confirmation.',
  'devices.read.loading': 'Reading the saved device list…',
  'devices.read.saved': 'Saved device list. This view does not confirm the current network state.',
  'devices.read.verified': 'Showing the device list from a verified network profile.',
  'devices.read.verifiedSaved':
    'The saved device list is shown; the verified reply did not provide a newer version.',
  'devices.read.unavailable':
    'Could not read a verified network profile. The saved device list remains visible.',
  'devices.read.noProfile':
    'No network profile was found. The saved device list remains visible; this does not confirm disconnection.',
  'devices.read.empty':
    'The verified profile lists no devices. This does not confirm an earlier disconnect request.',
  'devices.read.error': 'Could not read the saved device list. Return or try Refresh.',
  'network.save': 'Save draft',
  'network.activate': 'Activate checked settings',
  'network.activating': 'Activating checked settings…',
  'network.continue': 'Continue',
  'receive.title': 'Receive a profile',
  'receive.wizard_intro':
    'Show this code to a device with your profile and approve linking there. Other saved profiles stay on this phone.',
  'receive.network_title': 'Connect to receive your profile',
  'receive.network_intro':
    'Use the same network as your other device. Check the connection and activate it before creating the link code.',
  'wizard.receive': 'Receive my existing profile',
  'setup.finish': 'Finish device setup',
  'setup.running': 'Completing device enrollment and private-data synchronization…',
  'setup.complete': 'Pending device setup completed.',
  'setup.no_publish': 'Completing device setup does not publish profile edits.',
  'setup.review':
    'The original profile or link context needs a fresh comparison. Review it before trying again.',
  'setup.private_review':
    'Private data needs explicit recovery review. This action cannot choose how to restore it.',
  'setup.authorization':
    'Earlier device enrollment is uncertain. New approval from an authorized device is required; retry cannot restore authorization.',
  'setup.revoked': 'This device was revoked. It cannot restore its own authorization.',
  'setup.unlisted':
    'This device is absent from the verified device list. Approval from an authorized device is required.',
  'setup.selection': 'The selected fields need review. The original comparison is retained.',
  'setup.error':
    'Device setup could not be confirmed. The original attempt is retained; earlier steps may already have completed.',
  'receive.intro':
    'Configure Network first. Show this code to a device with your profile, then approve linking there. Existing profiles on this phone are preserved.',
  'receive.start': 'Create link code',
  'receive.source': 'Started from identity',
  'receive.qr': 'QR code for receiving this profile',
  'receive.expiry': 'Code expires: 2027-01-01 01:03 UTC',
  'receive.cancel': 'Stop waiting',
  'receive.open': 'Return to app',
  'receive.review': 'Review stored profiles',
  'receive.disposition':
    'Closing this result does not remove or repair stored profiles. Review them if installation could not be confirmed.',
  'receive.dismiss': 'Close this result',
  'receive.starting': 'Creating the public link code…',
  'receive.waiting': 'Waiting for approval on your other device.',
  'receive.receiving': 'Receiving the profile…',
  'receive.installing': 'Saving the received profile. Cancellation can no longer undo this step.',
  'receive.received': 'Profile received.',
  'receive.pending':
    'Enrollment and private-data synchronization may still require completion. Receiving the profile does not confirm that setup is finished.',
  'receive.cancelled': 'Waiting stopped. This does not remove any stored profile.',
  'receive.expired': 'This code has expired. Close this result before creating another code.',
  'receive.invalidated':
    'The original profile opening changed. Review the current profiles before starting again.',
  'receive.error': 'The last step could not be confirmed. This original attempt is retained.',
  'receive.reconcile':
    'Installation needs review. No rollback or successful completion is assumed.',
  'receive.admission': 'Installation was refused before reserving a destination.',
  'receive.reservation': 'Destination reservation failed. No exact destination was confirmed.',
  'receive.unconfirmed':
    'Installation completion is unknown. Review stored profiles before trying again.',
  'receive.destination':
    'Installation failed after reserving this destination. Review its actual state.',
  'language.name': 'English',
  'identity.persistence.title': 'Result',
  'identity.persistence.notice':
    'The identity change completed, but its persistence could not be confirmed. After restarting the app, check which identity is selected.',
  'identity.persistence.ack': 'Continue',
  'privacy.hint': 'These settings stay on this device and apply to this identity only.',
  'nav.back': 'Back',
  'storage.kind.system': 'Android Keystore',
  'storage.android': 'The key is kept in Android Keystore on this device.',
  'settings.active': 'Active · Android Keystore',
  'recovery.reason': "Its key is missing from this device's Keystore. No new key was created.",
  'lock.title': 'Protect the app',
  'lock.body': 'A PIN of 6 to 64 digits hides your identities and chats until you enter it.',
  'lock.recovery': 'The device screen lock recovers a forgotten PIN.',
  'lock.skip': 'Skip',
  'rename.label': 'Private contact name',
  'clear.text':
    'Messages and their kept originals will be removed from this device only. The other person keeps their copy.',
  'chat.cleared': 'Chat cleared on this device',
  'lock.set': 'Set PIN',
  'lock.biometric': 'Use device biometrics',
  'lock.pin': 'PIN',
  'lock.pinRepeat': 'Repeat the PIN',
  'lock.unlock.title': 'Enter your PIN',
  'lock.unlock': 'Unlock',
  'lock.wrong': 'Wrong PIN.',
  'lock.device': 'Use device screen lock',
  'lock.settings': 'App lock',
  'lock.mode.pin': 'App lock: PIN',
  'lock.current': 'Current PIN',
  'lock.change': 'Change PIN',
  'lock.turnOff': 'Turn off PIN',
  'settings.passed.hint': 'A pass hides someone from your search and Liked you. They are not told.',
  'settings.blocked.retry': 'Retry token revocation',
  'settings.blocked.none': 'Nobody is blocked.',
  'settings.blocked.hint':
    'A block hides someone from your search and chats and revokes their network tokens. They are not told. Retry the revocation if it is still pending.',
});
Object.assign(STRINGS.ru, {
  'owner.intro': 'На новом устройстве выберите «Получить профиль» и вставьте сюда его код.',
  'owner.identity': 'Идентичность, передающая полный контроль',
  'owner.preparing': 'Подготовка подтверждения. Приватные данные ещё не отправлялись.',
  'owner.prepareError': 'Не удалось подготовить подтверждение. Приватные данные не отправлялись.',
  'owner.sending': 'Отправка зашифрованных данных профиля. Дождитесь подтверждения.',
  'devices.this': 'Это устройство',
  'devices.revoking': 'Устройство отключается. Дождитесь подтверждения.',
  'devices.read.loading': 'Чтение сохранённого списка устройств…',
  'devices.read.saved': 'Сохранённый список устройств. Это не подтверждает текущее состояние сети.',
  'devices.read.verified': 'Показан список устройств из проверенного сетевого профиля.',
  'devices.read.verifiedSaved':
    'Показан сохранённый список: проверенный ответ не содержит более новой версии.',
  'devices.read.unavailable':
    'Не удалось прочитать проверенный сетевой профиль. Сохранённый список устройств остаётся на экране.',
  'devices.read.noProfile':
    'Сетевой профиль не найден. Сохранённый список остаётся на экране; это не подтверждает отключение.',
  'devices.read.empty':
    'В проверенном профиле нет устройств. Это не подтверждает прежний запрос отключения.',
  'devices.read.error': 'Не удалось прочитать сохранённый список. Вернитесь или обновите список.',
  'network.save': 'Сохранить черновик',
  'network.activate': 'Активировать проверенные настройки',
  'network.activating': 'Активация проверенных настроек…',
  'network.continue': 'Продолжить',
  'receive.title': 'Получить профиль',
  'receive.wizard_intro':
    'Покажите код устройству с вашим профилем и подтвердите привязку на нём. Другие сохранённые профили останутся на этом телефоне.',
  'receive.network_title': 'Подключитесь для получения профиля',
  'receive.network_intro':
    'Используйте ту же сеть, что и на другом устройстве. Проверьте и активируйте подключение перед созданием кода привязки.',
  'wizard.receive': 'Получить существующий профиль',
  'setup.finish': 'Завершить настройку устройства',
  'setup.running': 'Завершение регистрации устройства и синхронизации личных данных…',
  'setup.complete': 'Ожидающая настройка устройства завершена.',
  'setup.no_publish': 'Завершение настройки устройства не публикует правки анкеты.',
  'setup.review':
    'Исходный профиль или контекст привязки требуют нового сравнения. Проверьте его перед новой попыткой.',
  'setup.private_review':
    'Личные данные требуют явной проверки восстановления. Это действие не выбирает способ их восстановления.',
  'setup.authorization':
    'Результат прежней регистрации устройства неизвестен. Нужно новое подтверждение авторизованного устройства; повтор не восстанавливает авторизацию.',
  'setup.revoked': 'Это устройство отозвано. Оно не может восстановить собственную авторизацию.',
  'setup.unlisted':
    'Устройства нет в проверенном списке. Требуется подтверждение авторизованного устройства.',
  'setup.selection': 'Выбранные поля требуют проверки. Исходное сравнение сохранено.',
  'setup.error':
    'Настройка устройства не подтверждена. Исходная попытка сохранена; предыдущие шаги уже могли завершиться.',
  'receive.intro':
    'Сначала настройте сеть. Покажите код устройству с вашим профилем и подтвердите привязку на нём. Уже сохранённые профили этого телефона останутся.',
  'receive.start': 'Создать код привязки',
  'receive.source': 'Начато из профиля',
  'receive.qr': 'QR-код для получения профиля',
  'receive.expiry': 'Код действует до: 2027-01-01 01:03 UTC',
  'receive.cancel': 'Прекратить ожидание',
  'receive.open': 'Вернуться в приложение',
  'receive.review': 'Проверить сохранённые профили',
  'receive.disposition':
    'Закрытие результата не удаляет и не исправляет сохранённые профили. Проверьте их, если установка не подтверждена.',
  'receive.dismiss': 'Закрыть результат',
  'receive.starting': 'Создание открытого кода привязки…',
  'receive.waiting': 'Ожидание подтверждения на другом устройстве.',
  'receive.receiving': 'Получение профиля…',
  'receive.installing': 'Полученный профиль сохраняется. Отмена уже не может прервать этот шаг.',
  'receive.received': 'Профиль получен.',
  'receive.pending':
    'Регистрация устройства и синхронизация личных данных могут требовать завершения. Получение профиля ещё не подтверждает окончание настройки.',
  'receive.cancelled': 'Ожидание прекращено. Сохранённые профили не удаляются.',
  'receive.expired': 'Срок действия кода истёк. Закройте результат перед созданием нового кода.',
  'receive.invalidated':
    'Исходное открытие профиля изменилось. Проверьте текущие профили перед новым запуском.',
  'receive.error': 'Последний шаг не подтверждён. Исходная попытка сохранена.',
  'receive.reconcile':
    'Установку нужно проверить. Откат или успешное завершение не предполагаются.',
  'receive.admission': 'Установка отклонена до резервирования места для профиля.',
  'receive.reservation':
    'Не удалось зарезервировать место для профиля. Точное место не подтверждено.',
  'receive.unconfirmed':
    'Завершение установки неизвестно. Проверьте сохранённые профили перед новой попыткой.',
  'receive.destination':
    'Установка не удалась после резервирования этого места. Проверьте его текущее состояние.',
  'language.name': 'Русский',
  'identity.persistence.title': 'Результат',
  'identity.persistence.notice':
    'Изменение личности выполнено, но не удалось подтвердить его сохранение. После перезапуска приложения проверьте, какая личность выбрана.',
  'identity.persistence.ack': 'Продолжить',
  'privacy.hint': 'Эти настройки хранятся на этом устройстве и действуют только для этой личности.',
  'nav.back': 'Назад',
  'storage.kind.system': 'Хранилище ключей Android',
  'storage.android': 'Ключ хранится в хранилище ключей Android на этом устройстве.',
  'settings.active': 'Активна · Хранилище ключей Android',
  'recovery.reason': 'Ключа нет в хранилище ключей устройства. Новый ключ не создан.',
  'lock.title': 'Защитите приложение',
  'lock.body': 'PIN из 6–64 цифр скрывает ваши личности и чаты, пока вы его не введёте.',
  'lock.recovery': 'Забытый PIN восстанавливается блокировкой экрана устройства.',
  'lock.skip': 'Пропустить',
  'rename.label': 'Личное имя контакта',
  'clear.text':
    'Сообщения и сохранённые оригиналы удалятся только с этого устройства. У собеседника останется копия.',
  'chat.cleared': 'Переписка очищена на этом устройстве',
  'lock.set': 'Установить PIN',
  'lock.biometric': 'Использовать биометрию устройства',
  'lock.pin': 'PIN',
  'lock.pinRepeat': 'Повторите PIN',
  'lock.unlock.title': 'Введите PIN',
  'lock.unlock': 'Разблокировать',
  'lock.wrong': 'Неверный PIN.',
  'lock.device': 'Блокировка экрана устройства',
  'lock.settings': 'Блокировка приложения',
  'lock.mode.pin': 'Блокировка приложения: PIN',
  'lock.current': 'Текущий PIN',
  'lock.change': 'Сменить PIN',
  'lock.turnOff': 'Отключить PIN',
  'settings.passed.hint':
    'Пропуск скрывает человека из вашего поиска и списка «Вы понравились». Человек об этом не узнает.',
  'settings.blocked.retry': 'Повторить отзыв токенов',
  'settings.blocked.none': 'Никто не заблокирован.',
  'settings.blocked.hint':
    'Блокировка скрывает человека из вашего поиска и чатов и отзывает его сетевые токены. Человек об этом не узнает. Если отзыв ещё ожидается, повторите его.',
});
