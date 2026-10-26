# Клиент для macOS

Клиент `DYApp` для macOS — универсальное приложение (Apple silicon и Intel) для macOS 12 и новее.
Первая версия — минимальная оболочка: она запускается, отрисовывается и завершается. Профили,
поиск и чаты запланированы.

После создания или повторного открытия личности без активной сети форма подключения открывается
автоматически. Если сеть уже активна, открывается экран готовности, откуда можно снова открыть
форму. Ошибка чтения активного файла показывает повторную попытку и сохраняет личность.

У проекта нет Apple Developer ID. Опубликованный `DYApp-unsigned.zip` **не подписан и не
нотаризован**: это ровно то, что собрала нативная сборка macOS в CI, где также проходят модульные и
UI-тесты. Скачанным Mac его не откроет. Участник Apple Developer Program может собрать и подписать
копию из того же тега; подписанная копия — его, а не выпуск проекта.

## Откуда берётся ZIP

Плановый workflow `release-sizes` собирает Release на `macos-15` с `CODE_SIGNING_ALLOWED=NO`,
проверяет бандл (`tools/scripts/package-apple-release.py`: обе архитектуры, исполняемый файл, структура)
и сохраняет ZIP как артефакт `apple-macos-unsigned`. Готовые файлы в релизах с контрольной суммой
и происхождением запланированы.

## Сборка из исходников

Нужны Mac с Xcode 16, Rust 1.99 (`rustup`) и [XcodeGen](https://github.com/yonaskolb/XcodeGen)
2.46. Получите тег выпуска, соберите ядро на Rust и сгенерируйте проект Xcode:

```sh
git clone --branch v0.0.1 https://github.com/abbat/dyapp.git && cd dyapp
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bash tools/scripts/build-apple-core.sh macos release
xcodegen generate --spec clients/macos/project.yml --project clients/macos
xcodebuild -project clients/macos/DYApp.xcodeproj -scheme DYApp -configuration Release \
    -destination generic/platform=macOS -derivedDataPath build \
    CODE_SIGNING_ALLOWED=NO ONLY_ACTIVE_ARCH=NO ARCHS="arm64 x86_64" build
```

Приложение — `build/Build/Products/Release/DYApp.app`. Ядро на Rust слинковано статически,
вложенных фреймворков нет, поэтому одна подпись покрывает весь бандл.

## Запуск своей сборки

У локальной сборки нет флага карантина. Чтобы открыть её на Mac, где она собрана, подпишите её
ad hoc:

```sh
codesign --force --sign - build/Build/Products/Release/DYApp.app
open build/Build/Products/Release/DYApp.app
```

Неподписанный ZIP из CI запускается так же после `xattr -dr com.apple.quarantine DYApp.app`.
Делайте это, только если SHA-256 архива совпал с опубликованным: за эти байты не ручается никто,
кроме запуска CI.

## Подпись и нотаризация для других Mac

Для других Mac участник Apple Developer Program подписывает приложение своим Developer ID и
отправляет его Apple на нотаризацию
([Apple: нотаризация ПО для macOS](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)):

```sh
app=build/Build/Products/Release/DYApp.app
codesign --force --options runtime --timestamp \
    --sign "Developer ID Application: <Name> (<TEAMID>)" "$app"
codesign --verify --strict --verbose=2 "$app"
ditto -c -k --keepParent "$app" DYApp.zip
xcrun notarytool store-credentials dyapp --apple-id <id> --team-id <TEAMID>  # один раз
xcrun notarytool submit DYApp.zip --keychain-profile dyapp --wait
xcrun stapler staple "$app"
spctl --assess --type execute --verbose "$app"  # accepted, source=Notarized Developer ID
ditto -c -k --keepParent "$app" DYApp.zip       # копия со скрепкой для раздачи
```

Затем скопируйте ZIP на Mac, где приложения ещё не было, откройте его и проверьте запуск и
завершение. Публикуйте эту копию под своим именем: подпись на ней ваша.
