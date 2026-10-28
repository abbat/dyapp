# Клиент для Linux

Клиент `dyapp` для Linux — окно WebKitGTK. Первая версия — минимальная оболочка: она запускается,
отрисовывается и закрывается. Профили, поиск и чаты в окне запланированы.

## Первый запуск

Пока идентичности нет, окно объясняет, что это такое, и предлагает хранилище ключа: системное
хранилище учётных данных, выбранное заранее как рекомендуемое, если оно есть в сеансе (иначе
показана причина и заранее выбран файл), или файл в каталоге данных приложения с предупреждением о
копировании. Ничего не создаётся до нажатия **Создать идентичность**. Восстановление существующей
идентичности запланировано. Смена, добавление и удаление идентичности доступны только в настройках и
на экране восстановления ключа, который не открывается.

## Сборка

```sh
make prepare  # один раз: образы Docker
make deb      # → build/deb/dyapp_<version>_<arch>.deb и dyapp-<version>-<machine>-linux-gnu.tar.gz
```

Оба собираются в Debian 12, поэтому работают в Debian 12 и более новых системах с glibc 2.36 и
выше. `make deb` устанавливает каждый в чистый Debian 12 и открывает и закрывает окно под Xvfb.

Готовые файлы в релизах запланированы.

## Установка пакета

```sh
sudo apt install ./dyapp_<version>_<arch>.deb
```

`apt` ставит зависимости: WebKitGTK 4.1 (`libwebkit2gtk-4.1-0`) и GTK 3. Пакет устанавливает
`/usr/bin/dyapp`, пункт меню (`/usr/share/applications/dyapp.desktop`), значок и метаданные
AppStream (`/usr/share/metainfo/io.github.abbat.dyapp.metainfo.xml`). Название типа приложения в
меню и описание в каталоге программ переведены на русский; поле Debian `Description` остаётся
английским. Удаление: `sudo apt remove dyapp`.

## Установка из архива

В архиве `dyapp`, `dyapp.desktop`, `dyapp.png` и `LICENSE`. Сначала поставьте библиотеки (Debian и
Ubuntu: `sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0`), затем:

```sh
tar -xzf dyapp-<version>-<machine>-linux-gnu.tar.gz
./dyapp-<version>-<machine>-linux-gnu/dyapp
```

Для пункта меню скопируйте `dyapp` в `~/.local/bin/`, `dyapp.desktop` в
`~/.local/share/applications/` и `dyapp.png` в `~/.local/share/icons/hicolor/32x32/apps/`.
