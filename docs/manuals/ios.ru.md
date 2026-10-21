# Клиент для iOS

Клиент `DYApp` для iOS работает на iOS 14 и новее. Первая версия — минимальная оболочка: она
запускается, отрисовывается и закрывается. Профили, поиск и чаты запланированы.

У проекта нет учётной записи Apple Developer Program. Опубликованный `DYApp-unsigned.ipa` **не
подписан**, а iOS ставит только подписанные приложения, поэтому установить его в таком виде нельзя.
Это ровно то, что собрала нативная сборка в CI (`release-sizes` на `macos-15`, артефакт
`apple-ios-unsigned`); тестовый прогон CI (`ci.yml`) также выполняет модульные и UI-тесты в
симуляторе iOS. Чтобы пользоваться приложением, соберите и подпишите его своим Apple ID из того же
тега. Подписанная копия — ваша, а не выпуск проекта. Готовые файлы в релизах с контрольной суммой и
происхождением запланированы.

## Сборка из исходников

Нужны Mac с Xcode 16, Rust 1.99 (`rustup`) и [XcodeGen](https://github.com/yonaskolb/XcodeGen)
2.46. Получите тег выпуска, соберите ядро на Rust и сгенерируйте проект Xcode:

```sh
git clone --branch v0.0.1 https://github.com/abbat/dyapp.git && cd dyapp
rustup target add aarch64-apple-ios
bash tools/scripts/build-apple-core.sh ios-device release
xcodegen generate --spec clients/ios/project.yml --project clients/ios
```

## Установка на свой iPhone

Проще всего через Xcode: откройте `clients/ios/DYApp.xcodeproj`, выберите цель `DYApp` и в «Signing &
Capabilities»:

- включите «Automatically manage signing» и выберите свою команду. Подходит и бесплатный Apple
  ID; его профиль действует 7 дней;
- задайте свой идентификатор бандла, например `com.<you>.dyapp`.

В проекте стоит `CODE_SIGNING_ALLOWED = NO`, чтобы CI собирал без учётной записи: в «Build
Settings» поставьте `Yes` для цели `DYApp`. Подключите iPhone, включите на нём режим разработчика
(Настройки → Конфиденциальность и безопасность), выберите его целью запуска и нажмите Run.

## Подпись для нескольких устройств

Участник платной Apple Developer Program может заархивировать приложение и экспортировать его для
устройств, зарегистрированных в своей учётной записи
([Apple: раздача на зарегистрированные устройства](https://developer.apple.com/documentation/xcode/distributing-your-app-to-registered-devices)):

```sh
xcodebuild -project clients/ios/DYApp.xcodeproj -scheme DYApp -configuration Release \
    -destination generic/platform=iOS -archivePath build/DYApp.xcarchive \
    -allowProvisioningUpdates CODE_SIGNING_ALLOWED=YES CODE_SIGN_STYLE=Automatic \
    DEVELOPMENT_TEAM=<TEAMID> PRODUCT_BUNDLE_IDENTIFIER=com.<you>.dyapp archive
xcodebuild -exportArchive -archivePath build/DYApp.xcarchive -exportPath build/export \
    -exportOptionsPlist options.plist -allowProvisioningUpdates
```

Здесь `options.plist` — property list с `method` = `release-testing` и `teamID` = вашей
командой. Установите полученный `build/export/DYApp.ipa` через Finder или Apple Configurator и
проверьте запуск и закрытие на каждом устройстве.
