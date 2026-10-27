pub(super) const DESCRIPTIONS: &[(&str, &str)] = &[
    ("listen", "адреса, на которых узел принимает подключения"),
    ("external", "публичные адреса узла, если они известны"),
    (
        "seeds",
        "узлы для первого подключения: host:port или host:port/<peer-id>; IP или DNS",
    ),
    (
        "roles",
        "роли узла, например [\"store\", \"search\", \"media\"]",
    ),
    ("storage.dir", "каталог всех данных узла"),
    (
        "storage.key",
        "файл ключа узла; пусто: credential node.key из systemd, иначе dir/node.key",
    ),
    (
        "limits.message_ttl_hours",
        "срок хранения недоставленных сообщений",
    ),
    (
        "limits.attachment_retention_hours",
        "срок хранения вложений, которые получатель ещё не скачал",
    ),
    (
        "limits.media_cache_minutes",
        "срок хранения собранных из шардов файлов после последнего чтения",
    ),
    (
        "limits.profile_ttl_days",
        "срок хранения профиля после последнего подписанного запроса владельца",
    ),
    (
        "limits.profiles_max_mb",
        "предельный размер хранилища профилей",
    ),
    (
        "limits.messages_max_mb",
        "предельный размер хранилища почтовых ящиков",
    ),
    (
        "limits.signal_max_kb",
        "сигналы на ящик; самые старые вытесняются; минимум 2",
    ),
    (
        "limits.mailbox_pool_kb",
        "сообщения без токена на ящик при require_write_tokens; минимум 101",
    ),
    (
        "limits.tokenless_share_pct",
        "доля сообщений без токена во всём хранилище, % от messages_max_mb",
    ),
    (
        "limits.require_write_tokens",
        "сообщения без токена идут в отдельный пул, а не в 10 МиБ",
    ),
    (
        "limits.media_max_mb",
        "предельный размер хранилища медиафайлов",
    ),
    (
        "limits.media_per_owner_mb",
        "предельный размер медиафайлов одного пользователя",
    ),
    (
        "limits.min_free_mb",
        "запись останавливается, если на диске storage.dir меньше свободного места",
    ),
    (
        "limits.requests_per_second",
        "запросов в секунду от одного узла",
    ),
    (
        "limits.media_requests_per_second",
        "запросов медиа в секунду; ответ содержит до 1 МиБ",
    ),
    (
        "limits.bytes_per_second",
        "входящий и исходящий трафик узла в секунду; 0 = без ограничения",
    ),
    ("limits.max_connections", "общее число соединений"),
    (
        "limits.max_connections_per_peer",
        "соединений от одного узла",
    ),
    (
        "limits.max_connections_per_ip_group",
        "входящие ожидающие и установленные соединения; исходящие не учитываются",
    ),
    (
        "limits.max_streams",
        "одновременных потоков на соединение и протокол",
    ),
    (
        "limits.max_memory_mb",
        "лимит RSS: отклонять запросы с 80 %, закрывать при 100 %; 0 = без ограничения",
    ),
    (
        "limits.ip_group_requests_per_second",
        "запросов в секунду от группы IP; 1..100000000 (10-кратный грубый лимит)",
    ),
    ("limits.ipv4_prefix", "префикс группировки адресов IPv4"),
    ("limits.ipv6_prefix", "префикс группировки адресов IPv6"),
    (
        "limits.coarse_ipv4_prefix",
        "крупная группа с 10-кратным лимитом; 0..32",
    ),
    (
        "limits.coarse_ipv6_prefix",
        "крупная группа с 10-кратным лимитом; 0..128",
    ),
    (
        "limits.sender_puts_per_second",
        "сообщений в секунду от одного отправителя",
    ),
    (
        "limits.strikes_to_ban",
        "число отклонённых запросов и плохих подписей до бана узла",
    ),
    ("limits.ban_minutes", "срок действия бана в минутах"),
    (
        "search.max_profile_bytes",
        "лимит записи клиента; 1..65536 байт; сеть принимает 65536",
    ),
    (
        "search.max_bytes",
        "размер profiles-idx.db; сверх лимита крупнейшая локация теряет ключи",
    ),
    (
        "search.newcomer_days",
        "сколько дней ключ считается новым; 1..365",
    ),
    (
        "search.newcomer_share",
        "доля max_bytes для новых участников; (0, 1]",
    ),
    (
        "search.max_scan_rows",
        "строк на один поисковый запрос; после лимита ответ неполный",
    ),
    (
        "search.puzzle_wait_ms",
        "медианное ожидание в очереди для включения задачи клиенту; > 0 мс",
    ),
    (
        "search.max_puzzle_bits",
        "верхняя граница сложности задачи; 12..32 бита",
    ),
    (
        "search.topics",
        "темы gossip (0..15); пусто = все 16; публичная сеть: пусто",
    ),
    (
        "search.min_profile_pow_bits",
        "минимум Argon2id для профилей в store/search; 0..20 бит",
    ),
    (
        "search.min_heartbeat_pow_bits",
        "минимум Argon2id для ежедневного heartbeat; 0 = нет, 0..16",
    ),
    (
        "media.shard_threshold",
        "порог хранения целой копии в байтах; 1..1048576",
    ),
    (
        "turn.urls",
        "например [\"turn:turn.example.org:3478\", \"turns:turn.example.org:5349\"]",
    ),
    (
        "turn.secret",
        "предпочтительнее secret_file; значение опции видно в ps",
    ),
    (
        "turn.secret_file",
        "файл с секретом; один завершающий перевод строки удаляется, например credential systemd",
    ),
    (
        "turn.credential_minutes",
        "срок действия выдаваемых паролей в минутах",
    ),
    ("relay.max_reservations", "резерваций relay одновременно"),
    ("relay.max_reservations_per_peer", "резерваций одного узла"),
    (
        "relay.reservation_minutes",
        "срок резервации до продления, в минутах",
    ),
    ("relay.max_circuits", "цепей relay одновременно"),
    (
        "relay.max_circuits_per_peer",
        "цепей к одному узлу или от него",
    ),
    ("relay.circuit_seconds", "время жизни одной цепи в секундах"),
    (
        "relay.circuit_kb",
        "КиБ, которые цепь передаёт в каждую сторону до закрытия",
    ),
    (
        "maintenance.interval_minutes",
        "интервал очистки просроченных данных, сохранения узлов и повторного дозвона до seeds",
    ),
    (
        "maintenance.vacuum_pages",
        "страниц диска по 4 КиБ, освобождаемых за цикл в каждом хранилище и индексе",
    ),
    (
        "maintenance.io_mb_per_s",
        "общий бюджет записи при фоновом перестроении, МиБ/с; > 0",
    ),
    (
        "network.name",
        "пусто = публичная сеть; для именованной сети нужна привязка хранилища",
    ),
    (
        "network.deny_dial",
        "запрет соединений даже после DNS; [] = отключено",
    ),
    (
        "network.id_pow_bits",
        "сложность доказательства работы для ключей узлов; публичной сети нужно 22",
    ),
    (
        "network.distinct_outbound_groups",
        "не более одного маршрутизируемого узла на /16 (IPv6 /32) в корзине",
    ),
    (
        "network.guard_dials",
        "запрет соединений с частными IP и лимит на /24 (IPv6 /48)",
    ),
    (
        "network.storage_trust_minutes",
        "когда новому узлу начинают выдавать копии данных",
    ),
    (
        "network.share_deny_list",
        "отвечать другим узлам на запросы списка запретов",
    ),
    (
        "network.accept_deny_lists",
        "получать списки запретов других узлов; хранить, но не применять",
    ),
    (
        "metrics.port",
        "слушать [::1] и 127.0.0.1 на этом порту; 0 = отключено",
    ),
];
