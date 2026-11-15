# Резерв схемы групповых чатов на клиенте

Дата: 2026-11-15. Статус: дизайн согласован, реализация не начата.

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
- Участники читают топик по курсору (`seq`). ACK сообщения не удаляет, они уходят по сроку.
- Commit и Welcome по-прежнему идут в личные ящики участников.

## Порядок изменений

- Свойства группы, роли и состав меняются только MLS-коммитами и версионируются `epoch`.
- Карточку участника (имя и фото в группе) публикует сам участник. Конфликты решает LWW
  по (hlc, устройство, operation ID), как у правок сообщений.
- Локальные флаги группы (mute, избранное) синхронизируются между своими устройствами
  так же, как одноимённые поля `peers`: значение, `*_hlc`, `*_device`.

## Схема

```sql
CREATE TABLE `chat_groups` (
    `group_id` BLOB PRIMARY KEY NOT NULL CHECK (length(`group_id`) > 0),
    -- 1 приглашён, 2 участник, 3 вышел, 4 удалён
    `state` INTEGER NOT NULL CHECK (`state` IN (1, 2, 3, 4)),
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
    -- 1 приглашён, 2 участник, 3 вышел, 4 удалён
    `state` INTEGER NOT NULL CHECK (`state` IN (1, 2, 3, 4)),
    -- 1 добавлен участником, 2 вошёл по ссылке
    `source` INTEGER NOT NULL CHECK (`source` IN (1, 2)),
    `added_by` TEXT,
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

-- курсор чтения топика эпохи на каждом узле
CREATE TABLE `chat_group_cursors` (
    `group_id` BLOB NOT NULL CHECK (length(`group_id`) > 0),
    `topic` BLOB NOT NULL CHECK (length(`topic`) = 32),
    `node` TEXT NOT NULL CHECK (length(`node`) > 0),
    `epoch` INTEGER NOT NULL CHECK (`epoch` >= 0),
    `seq` INTEGER NOT NULL CHECK (`seq` >= 0),
    PRIMARY KEY (`group_id`, `topic`, `node`)
) STRICT;

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

## Безопасность

- `mls_group_state.sealed` запечатывается тем же ключом, что и `mls_state`.
- Адрес топика хранится только в `chat_group_cursors`, больше нигде.
- Карточка участника группы отделена от анкеты знакомств: опубликованное в группе имя
  не раскрывает профиль.
- `docs/security/privacy.md` обновляем вместе с фичей, не в этом резерве: пока код
  таблицы не пишет, сказать о новых данных нечего.

## Тест

Один сценарный тест хранилища (около 40 строк). Корректные строки во всех четырёх
таблицах принимаются. Отвергаются: роль 4, имя группы длиной 65 байт, закреп с одной
NULL-половиной, второй действующий владелец, топик длиной 31 байт, отрицательный `seq`.

## Документация

Четыре строки в каталоге таблиц `docs/design/client-local-storage.md` с пометкой
«зарезервировано, код пока не использует».

## Объём

Около 80 строк SQL, 40 строк теста и 15 строк документации: один коммит меньше 400 строк.
Bootstrap не трогаем.

## Beads

- P0-блокер v0.0.1: резерв схемы групповых чатов, по образцу резерва схемы реакций.
  Описание на уровне архитектора, детали — в поле `design`.
- Эпик после релиза «Групповые чаты»: доставка через топик эпохи, таблица топиков
  на bootstrap через `IF NOT EXISTS`, совместимость версий узлов при репликации,
  MLS-состояние больших групп, карточки участников, отсутствие отметок прочтения
  в группах. Открытые вопросы: ссылки-приглашения и кто может ими пользоваться.

## Вне объёма

Код групповых чатов, wire-формат, UI, изменения bootstrap, отметки прочтения в группах.
