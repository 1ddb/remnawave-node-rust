# Remnawave Node на Rust

[English](README.md) | [Русский](README.ru.md)

Rust-реимплементация [`remnawave/node`](https://github.com/remnawave/node),
совместимая по API с `node` 3.4.6 (upstream-коммит `52d0f1c`). Полноценная
замена агента, которым управляет панель Remnawave.

## Node.js vs Rust

| Метрика | [remnawave/node](https://github.com/remnawave/node) | 1ddb/remnawave-node-rust |
|---|---|---|
| Язык | TypeScript / Node.js | Rust |
| Память | ~107 MiB | ~6 MiB |
| Образ, диск | 510 MB | 177 MB |
| Образ, контент | 124 MB | 87.9 MB |
| CPU в простое | 0.0% | 0.0% |
| xray-core | отдельный бинарник | отдельный бинарник |

*Сравнение без учёта xray-core.*

Rust-агент потребляет примерно **в 17 раз меньше резидентной памяти**, чем
Node.js-версия, а Docker-образ примерно **в 2.9 раза меньше на диске** и **в 1.4
раза меньше по размеру контента**.

## Возможности

- Публичный HTTPS API с mutual TLS, опциональной SNI-проверкой и JWT (RS256)
  на всех маршрутах.
- Внутренний HTTP API на абстрактном unix-сокете с токен-аутентификацией.
- Супервизор Xray Core: запуск процесса, отслеживание падений, экспоненциальный
  backoff (1–30 с) и запись логов в `/var/log/xray/current`.
- gRPC-клиент Xray API, обработчики пользователей, статистика, geocheck,
  плагины и nftables.
- Кастомное ядро Xray и geodata с проверкой SHA-256 и атомарной установкой.
- Встроенный CLI для дампа конфигурации и работы с сокетами.

## Требования

- Linux с nftables и абстрактными unix-сокетами.
- Debian 12+ или Ubuntu 22.04+ для `.deb`-пакета.
- `NET_ADMIN` для nftables и `/dev/net/tun` для TUN-инбаундов.
- Права на запись в `/usr/local/bin`, `/var/log/xray`, `/run` и
  `/usr/local/share`.

## Миграция

Агент полностью заменяет Node.js-версию: существующий `.env` (`NODE_PORT`,
`SECRET_KEY` и опциональные переменные) менять не нужно.

### Docker Compose

Замените `image` в `docker-compose-prod.yml`:

```yaml
services:
  remnanode:
    image: ghcr.io/1ddb/remnawave-node-rust:latest
```

```bash
docker compose pull
docker compose up -d
```

Для новой установки сначала скопируйте [`docker-compose-prod.yml`](docker-compose-prod.yml)
и [`.env.sample`](.env.sample); `SECRET_KEY` берётся в настройках ноды в панели
или при её создании.

### Debian-пакет (опционально)

```bash
curl -fsSLO "https://github.com/1ddb/remnawave-node-rust/releases/download/v3.4.6/rw-node_3.4.6-1_$(dpkg --print-architecture).deb"
sudo apt install "./rw-node_3.4.6-1_$(dpkg --print-architecture).deb"
sudoedit /etc/rw-node/env   # вставьте SECRET_KEY из панели
sudo docker stop remnanode || true   # освободит порт, если старый контейнер есть
sudo systemctl enable --now rw-node
```

Переходите с Docker? Выполните `docker compose down` в каталоге старой установки,
чтобы удалить остановленный контейнер.

Пакет включает ядро Xray, геоданные, базу ASN и geocheck.

## Конфигурация

| Переменная | Обязательна | По умолчанию | Описание |
| --- | --- | --- | --- |
| `NODE_PORT` | да | — | Порт HTTPS API. |
| `SECRET_KEY` | да | — | Base64-encoded JSON с сертификатами и JWT public key из панели. |
| `SNI_VERIFICATION` | нет | `false` | Проверять вычисленный SNI при TLS-хендшейке. |
| `DISABLE_HASHED_SET_CHECK` | нет | `false` | Отключить проверку изменений hashed-set. |
| `NFTABLES_LOGGING` | нет | `true` | Включить логирование правил nftables. |
| `NFTABLES_ACCEPT_REPLY_TRAFFIC` | нет | `false` | Принимать reply-трафик в правилах nftables. |
| `NFTABLES_TABLE_NAME` | нет | `remnanode` | Имя таблицы nftables; для IPv6 используется суффикс `6`. |
| `SUPPRESS_DUPLICATE_INSTANCE_WARNING` | нет | `false` | Не предупреждать о втором node в том же network namespace. |
| `XRAY_LOCATION_ASSET` | нет | `/usr/local/share/xray` | Каталог geodata-ассетов Xray. |
| `RUST_LOG` | нет | `info` | Фильтр `tracing` (`debug` при `NODE_ENV=development`). |
| `NODE_ENV` | нет | `production` | Укажите `development` для debug-логов. |
| `INTERNAL_REST_TOKEN` | нет | генерируется | Токен внутреннего API. |
| `INTERNAL_SOCKET_PATH` | нет | генерируется | Абстрактный unix-сокет внутреннего API. |
| `XTLS_API_SOCKET_PATH` | нет | генерируется | Абстрактный unix-сокет gRPC API Xray. |

`INTERNAL_REST_TOKEN`, `INTERNAL_SOCKET_PATH` и `XTLS_API_SOCKET_PATH`
генерируются при первом запуске и сохраняются в `/run/rw-node.env` с правами
`0600`, чтобы CLI и перезапущенный процесс использовали те же значения.

Готовый шаблон — в [`.env.sample`](.env.sample).

## CLI

CLI доступен как `rw-node cli <команда>` или через симлинк `cli` в контейнере.
Для `.deb`-установки сначала загрузите окружение сервиса:
`source /etc/rw-node/env`.

```bash
rw-node cli --help
rw-node cli --dump-config
rw-node cli --dump-config-raw
rw-node cli --kill-sockets 203.0.113.10
```

## Runtime-файлы

| Путь | Назначение |
| --- | --- |
| `/usr/local/bin/rw-core` | Симлинк на активное ядро Xray. |
| `/usr/local/bin/xray` | Штатное ядро Xray. |
| `/usr/local/bin/xray-custom` | Установленное кастомное ядро Xray. |
| `/usr/local/bin/.rw-core.json` | Метаданные установленного кастомного ядра. |
| `/var/log/xray/current` | Вывод процесса Xray. |
| `/run/rw-node.env` | Сгенерированные внутренние токены и имена сокетов. |
| `/usr/local/share/xray` | `geoip.dat` и `geosite.dat`. |
| `/usr/local/share/asn/asn-prefixes.lmdb` | База префиксов ASN. |

## Разработка

Инструкции по сборке и структура проекта: [`DEVELOPMENT.md`](DEVELOPMENT.md)
(на английском).

## Лицензия

AGPL-3.0-only. См. [`LICENCE`](LICENCE).
