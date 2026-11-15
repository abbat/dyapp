# Резерв схемы групповых чатов на клиенте

Дата: 2026-11-15. Статус: дизайн согласован и прошёл стресс-тест, реализация не начата.

## Цель

Групповые чаты сделаем после v0.0.1. Таблицы для них должны попасть в релиз, тогда
реализация фичи обойдётся без миграции базы клиента. В v0.0.1 код таблицы не использует:
он их создаёт, и только.

## Почему резерв нужен на клиенте и не нужен на bootstrap

- Клиент создаёт схему целиком только при `user_version = 0` (`needs_schema` в
  `rust/client/src/store.rs`, VERSION = 1). После релиза любая новая таблица или колонка
  требует поднять версию и написать миграцию. Поэтому резервировать надо сейчас.
- Bootstrap при каждом открытии выполняет `CREATE TABLE IF NOT EXISTS`
  (`rust/bootstrap/src/storage.rs`). Таблица, добавленная позже, миграции не требует.
  На bootstrap ничего не резервируем.
- Если при реализации фичи окажется, что какой-то CHECK неудачен, таблица ещё пуста:
  её пересоздание безопасно, но всё равно станет первым шагом миграции. Цель резерва —
  чтобы такого шага не понадобилось.

## Что переиспользуем без изменений

Эти таблицы уже адресуются `group_id` и годятся для группы как есть: `chat_messages`,
`chat_operations`, `chat_attachments`, `chat_cleared`. В клиентской схеме нет FOREIGN KEY,
так что сообщения не зависят от того, в каком каталоге лежит группа. В `inbox` и `outbox`
ключ — 32-байтовый mailbox, и 32-байтовый топик группы туда помещается.

`conversation_groups` не меняется: в ней по-прежнему чаты 1:1 и группа своих устройств.
Правило `docs/design/client-local-storage.md`, которое отвергает членство в чужих группах,
тоже остаётся в силе до реализации фичи.

## Модель доставки (принята, реализуется вместе с фичей)

Группа может вырасти до 1000 участников, и рассылать каждое сообщение каждому участнику
по отдельности слишком дорого по трафику. Пользователь принимает принцип «что знают
трое — знает весь мир»: групповой чат по определению менее секретен, чем 1:1.

- Одно сообщение — один MLS-шифртекст в топик группы на bootstrap.
- Адрес топика выводится из exporter secret MLS текущей эпохи и меняется на каждой эпохе.
- Участники читают топик по курсору (`seq`) на каждом узле. ACK сообщения не удаляет,
  они уходят по сроку. Курсор удаляется, когда срок топика на узлах истёк. Дубликаты
  с разных узлов отсекаются по ID сообщения.
- Commit и Welcome по-прежнему идут в личные ящики участников.

## Порядок изменений

- Свойства группы, роли, состав и ссылки-приглашения меняются только MLS-коммитами
  и версионируются `epoch`.
- Карточку участника (имя и фото в группе) публикует сам участник. Конфликты решает LWW
  по (hlc, устройство, operation ID), как у правок сообщений.
- Локальные флаги группы (mute, избранное) синхронизируются между своими устройствами
  так же, как одноимённые поля `peers`: значение, `*_hlc`, `*_device`.

## Схема

Состояния членства одинаковы для `chat_groups.state` (моё членство) и
`chat_group_members.state`: 1 приглашён, 2 участник, 3 вышел, 4 удалён (может вернуться),
5 ждёт одобрения входа по ссылке, 6 забанен (ни приглашение, ни ссылка не пускают).

```sql
CREATE TABLE `chat_groups` (
    `group_id` BLOB PRIMARY KEY NOT NULL CHECK (length(`group_id`) > 0),
    `state` INTEGER NOT NULL CHECK (`state` IN (1, 2, 3, 4, 5, 6)),
    `epoch` INTEGER NOT NULL CHECK (`epoch` >= 0),
    `name` TEXT NOT NULL DEFAULT '' CHECK (length(CAST(`name` AS BLOB)) <= 64),
    `description` TEXT NOT NULL DEFAULT '' CHECK (length(CAST(`description` AS BLOB)) <= 1024),
    -- дескриптор вложения, как в chat_attachments
    `avatar` BLOB CHECK (`avatar` IS NULL OR length(`avatar`) BETWEEN 1 AND 2048),
    `pinned_author` TEXT,
    `pinned_message` BLOB CHECK (`pinned_message` IS NULL OR length(`pinned_message`) = 16),
    -- битовая маска прав; неизвестный бит отвергает изменение (проверяет код)
    `permissions` INTEGER NOT NULL DEFAULT 0 CHECK (`permissions` BETWEEN 0 AND 4294967295),
    `muted` INTEGER NOT NULL DEFAULT 0 CHECK (`muted` IN (0, 1)),
    `muted_hlc` INTEGER NOT NULL DEFAULT 0,
    `muted_device` BLOB NOT NULL DEFAULT x'',
    `favorite` INTEGER NOT NULL DEFAULT 0 CHECK (`favorite` IN (0, 1)),
    `favorite_hlc` INTEGER NOT NULL DEFAULT 0,
    `favorite_device` BLOB NOT NULL DEFAULT x'',
    CHECK ((`pinned_author` IS NULL) = (`pinned_message` IS NULL))
) STRICT;

CREATE TABLE `chat_group_members` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `peer_id` TEXT NOT NULL CHECK (length(`peer_id`) > 0),
    -- 1 владелец, 2 админ, 3 участник
    `role` INTEGER NOT NULL CHECK (`role` IN (1, 2, 3)),
    `state` INTEGER NOT NULL CHECK (`state` IN (1, 2, 3, 4, 5, 6)),
    -- 1 добавлен участником, 2 вошёл по ссылке
    `source` INTEGER NOT NULL CHECK (`source` IN (1, 2)),
    `added_by` TEXT,
    -- ссылка, по которой вошёл; число использований ссылки считается по этой колонке
    `via_link` BLOB CHECK (`via_link` IS NULL OR (length(`via_link`) = 16 AND `source` = 2)),
    `since_epoch` INTEGER NOT NULL CHECK (`since_epoch` >= 0),
    -- карточка участника; NULL, пока он её не опубликовал
    `display_name` TEXT CHECK (`display_name` IS NULL OR length(CAST(`display_name` AS BLOB)) <= 64),
    `photo` BLOB CHECK (`photo` IS NULL OR length(`photo`) BETWEEN 1 AND 2048),
    `card_hlc` INTEGER NOT NULL DEFAULT 0,
    `card_device` BLOB NOT NULL DEFAULT x'',
    `card_operation` BLOB CHECK (`card_operation` IS NULL OR length(`card_operation`) = 16),
    PRIMARY KEY (`group_id`, `peer_id`)
) STRICT;

-- не больше одного действующего владельца
CREATE UNIQUE INDEX `chat_group_owner` ON `chat_group_members` (`group_id`)
    WHERE `role` = 1 AND `state` = 2;

-- блокировка, общие группы и удаление профиля ищут по собеседнику
CREATE INDEX `chat_group_members_peer` ON `chat_group_members` (`peer_id`);

-- ссылки-приглашения; секрет запечатан, счётчика использований нет (см. via_link)
CREATE TABLE `chat_group_links` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `link_id` BLOB NOT NULL CHECK (length(`link_id`) = 16),
    `sealed_secret` BLOB NOT NULL CHECK (length(`sealed_secret`) BETWEEN 1 AND 256),
    `created_by` TEXT NOT NULL CHECK (length(`created_by`) > 0),
    `created_epoch` INTEGER NOT NULL CHECK (`created_epoch` >= 0),
    `revoked_epoch` INTEGER CHECK (`revoked_epoch` IS NULL OR `revoked_epoch` >= `created_epoch`),
    `expires_at_ms` INTEGER CHECK (`expires_at_ms` IS NULL OR `expires_at_ms` > 0),
    `max_uses` INTEGER CHECK (`max_uses` IS NULL OR `max_uses` >= 1),
    -- 1: вошедший по ссылке ждёт одобрения админа (состояние 5)
    `approval` INTEGER NOT NULL DEFAULT 0 CHECK (`approval` IN (0, 1)),
    PRIMARY KEY (`group_id`, `link_id`)
) STRICT;

-- курсор чтения топика эпохи на каждом узле
CREATE TABLE `chat_group_cursors` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `topic` BLOB NOT NULL CHECK (length(`topic`) = 32),
    `node` TEXT NOT NULL CHECK (length(`node`) > 0),
    `epoch` INTEGER NOT NULL CHECK (`epoch` >= 0),
    `seq` INTEGER NOT NULL CHECK (`seq` >= 0),
    PRIMARY KEY (`group_id`, `topic`, `node`)
) STRICT;

-- сообщения, которые упоминают меня или отвечают мне; ключ как у chat_messages,
-- чтобы признак не потребовал новой колонки в таблице с данными
CREATE TABLE `chat_group_mentions` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `author` TEXT NOT NULL CHECK (length(`author`) > 0),
    `message_id` BLOB NOT NULL CHECK (length(`message_id`) = 16),
    `unread` INTEGER NOT NULL DEFAULT 1 CHECK (`unread` IN (0, 1)),
    PRIMARY KEY (`group_id`, `author`, `message_id`)
) STRICT;

-- системные строки ленты («Аня добавила Борю»)
CREATE TABLE `chat_group_events` (
    `event_order` INTEGER PRIMARY KEY AUTOINCREMENT,
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `epoch` INTEGER NOT NULL CHECK (`epoch` >= 0),
    -- вид события; список видов ведёт код, диапазон оставляет место новым без миграции
    `kind` INTEGER NOT NULL CHECK (`kind` BETWEEN 1 AND 255),
    `actor` TEXT NOT NULL CHECK (length(`actor`) > 0),
    `target` TEXT CHECK (`target` IS NULL OR length(`target`) > 0),
    -- arrival_order последнего сообщения группы в момент применения; 0, если сообщений нет
    `after_arrival` INTEGER NOT NULL CHECK (`after_arrival` >= 0),
    `at_ms` INTEGER NOT NULL CHECK (`at_ms` > 0),
    -- новое значение (имя, роль и т.п.), если событию оно нужно
    `detail` BLOB CHECK (`detail` IS NULL OR length(`detail`) BETWEEN 1 AND 1024)
) STRICT;

CREATE INDEX `chat_group_events_feed` ON `chat_group_events` (`group_id`, `after_arrival`);

-- MLS-состояние каждой группы отдельно: дерево на 1000 участников занимает около
-- 0.5 МБ, и переписывать его вместе с общим mls_state на каждое сообщение нельзя
CREATE TABLE `mls_group_state` (
    `group_id` BLOB PRIMARY KEY NOT NULL CHECK (length(`group_id`) > 0),
    `sealed` BLOB NOT NULL,
    `revision` INTEGER NOT NULL CHECK (`revision` >= 0)
) STRICT;
```

`user_version` остаётся 1: таблицы входят в начальную схему до релиза.

## Инварианты, которые проверяет код (не CHECK)

- `group_id` лежит либо в `conversation_groups`, либо в `chat_groups`, не в обеих.
- Лимит участников (например, 1000) — константа ядра.
- Изменения группы применяются только после проверенного MLS-коммита. Отправителя берём
  из MLS, никогда из тела сообщения.
- Неизвестный бит `permissions` — изменение отвергается целиком.
- Забаненного (состояние 6) не впускают ни приглашение, ни ссылка, пока бан не снят
  коммитом.
- Ссылка не пускает после `revoked_epoch`, после `expires_at_ms` и после `max_uses`
  входов (считаются строки участников с этим `via_link`).
- Удаление сообщения и очистка чата удаляют его строку в `chat_group_mentions`; очистка
  чата удаляет и события группы в `chat_group_events`.
- События и сообщения склеиваются в ленту по `after_arrival`, при равенстве — по
  `event_order`. Неизвестный `kind` показывается нейтральной строкой «изменения в группе».
- MLS-состояние группы и запись в outbox фиксируются одной транзакцией, как в
  `stage_outbound`. Хранилище OpenMLS для Device при этом надо разделить по группам
  (входит в эпик).

## Безопасность

- `mls_group_state.sealed` запечатывается тем же ключом, что и `mls_state`, с AAD
  `(group_id, revision)`; `chat_group_links.sealed_secret` — с AAD `(group_id, link_id)`.
  Образец привязки — `aad` в `rust/client/src/tokens.rs`: запечатанный блоб нельзя
  переставить в чужую строку.
- Адрес топика хранится только в `chat_group_cursors`, больше нигде.
- Карточка участника группы отделена от анкеты знакомств: опубликованное в группе имя
  не раскрывает профиль.
- Резерв не добавляет поверхности атаки: таблицы пусты, код их не читает, правило
  против членства в чужих группах остаётся.
- `docs/security/privacy.md` обновляем вместе с фичей, не в этом резерве: пока код
  таблицы не пишет, сказать о новых данных нечего.

## Тест

Один сценарный тест хранилища (около 50 строк). Корректные строки во всех семи таблицах
принимаются. Отвергаются: роль 4, состояние 7, имя группы длиной 65 байт, закреп с одной
NULL-половиной, второй действующий владелец, топик длиной 31 байт, отрицательный `seq`,
`link_id` длиной 15 байт, `via_link` при `source = 1`, `kind` 0, `message_id` упоминания
длиной 15 байт.

## Документация

Семь таблиц и три индекса в каталоге таблиц `docs/design/client-local-storage.md`
с пометкой «зарезервировано, код пока не использует».

## Объём

Около 110 строк SQL, 50 строк теста и 20 строк документации: один коммит меньше 400 строк.
Bootstrap не трогаем.

## Beads

- P0-блокер v0.0.1: резерв схемы групповых чатов, по образцу резерва схемы реакций.
  Описание на уровне архитектора, детали — в поле `design`.
- Эпик после релиза «Групповые чаты»: доставка через топик эпохи, таблица топиков
  на bootstrap через `IF NOT EXISTS`, совместимость версий узлов при репликации,
  MLS-состояние больших групп и разделение хранилища OpenMLS, карточки участников,
  ссылки-приглашения, бан, упоминания, системные события, отсутствие отметок прочтения
  в группах. Открытые вопросы: кому вошедший по ссылке отправляет запрос и как секрет
  ссылки попадает к нему.

## Вне объёма

Код групповых чатов, wire-формат, UI, изменения bootstrap, отметки прочтения в группах.

## Stress Test Results: резерв схемы групповых чатов

### Resolved Decisions

- Таблицы создаём сейчас, а не потом: пересоздание пустой таблицы безопасно, но это уже
  миграция, а резерв нужен ровно для того, чтобы миграций не было.
- Ссылки-приглашения получают свою таблицу `chat_group_links`. Использования считаются
  по `via_link` участников, отдельного счётчика нет, поэтому рассинхрона быть не может.
- Индекс `chat_group_members_peer` нужен блокировке, общим группам и удалению профиля.
- Курсоры удаляются по сроку топика, дубликаты с разных узлов отсекаются по ID сообщения.
- Запечатанные блобы привязаны AAD к своей строке, по образцу `tokens.rs`.
- Резерв не добавляет поверхности атаки.
- Бан — отдельное состояние 6, отличное от «удалён».
- Упоминания хранятся в отдельной таблице, не в колонке `chat_messages`.
- Системные события хранятся в своей таблице; место в ленте задаёт `after_arrival`,
  а `kind` задан диапазоном.

### Changes Made

- Добавлены таблицы `chat_group_links`, `chat_group_mentions`, `chat_group_events`
  и индексы `chat_group_members_peer`, `chat_group_events_feed`.
- В `chat_group_members` добавлена колонка `via_link`; в обоих `state` добавлены
  состояния 5 (ждёт одобрения) и 6 (забанен).
- Дописаны инварианты, примечания по безопасности, проверки теста; пересчитаны объём
  и документация.

### Deferred / Parking Lot

- Сетевая логика входа по ссылке: кому идёт запрос и как передаётся секрет. Решается
  в эпике и на схему не влияет.

### Confidence Assessment

- Overall: Medium-High.
- Areas of concern: схема фиксируется до wire-формата и UI. Если фича потребует
  сущность, которую мы не предусмотрели, понадобится миграция, но уже только с
  добавлением новой пустой таблицы.
