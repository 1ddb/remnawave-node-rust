# Remnawave Node (Rust)

Rust-реимплементация [`remnawave/node`](https://github.com/remnawave/node),
совместимая по API с `node` 3.4.3 (upstream-коммит `2ad154f`). Это агент, который
запускается на каждой ноде и управляется панелью Remnawave.

## Node.js vs Rust

Полноценная замена оригинального Node.js-агента:

| Метрика | Node.js (upstream) | Rust (этот проект) |
|---|---|---|
| Язык | TypeScript / Node.js | Rust |
| Память | ~107 MiB | ~6 MiB |
| Образ, диск | 510 MB | 177 MB |
| Образ, контент | 124 MB | 87.9 MB |
| CPU в простое | 0.0% | 0.0% |

Rust-агент потребляет примерно **в 17 раз меньше резидентной памяти**, чем
Node.js-версия, а Docker-образ примерно **в 2.9 раза меньше на диске** и **в 1.4
раза меньше по размеру контента**.

## Возможности

- Публичный HTTPS API с mutual TLS, опциональной SNI-проверкой и JWT (RS256)
  на всех маршрутах.
- Внутренний HTTP API на абстрактном unix-сокете с токен-аутентификацией.
- Супервизор Xray Core: запуск процесса, отслеживание падений, экспоненциальный
  backoff (1–30 с) и запись логов в `/var/log/xray/current`.
- gRPC-клиент Xray API через абстрактный unix-сокет.
- Обработчики пользователей, статистика онлайна и трафика, geocheck, плагины и
  nftables.
- Кастомное ядро Xray и geodata с проверкой SHA-256 и атомарной установкой.
- Встроенный CLI для дампа конфигурации и работы с сокетами.

## Требования

- Linux с поддержкой абстрактных unix-сокетов и nftables.
- Rust 1.85 или новее.
- Привилегии во время работы:
  - `NET_ADMIN` для nftables;
  - `/dev/net/tun` для TUN-инбаундов;
  - права на запись в `/usr/local/bin`, `/var/log/xray`, `/run` и
    `/usr/local/share`.

## Конфигурация

| Переменная | Обязательна | По умолчанию | Описание |
| --- | --- | --- | --- |
| `NODE_PORT` | да | — | Порт HTTPS API. |
| `SECRET_KEY` | да | — | Base64-encoded JSON из панели. |
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

`SECRET_KEY` — это base64-encoded JSON со следующими полями:

```json
{
  "caCertPem": "-----BEGIN CERTIFICATE-----\n...",
  "jwtPublicKey": "-----BEGIN PUBLIC KEY-----\n...",
  "nodeCertPem": "-----BEGIN CERTIFICATE-----\n...",
  "nodeKeyPem": "-----BEGIN PRIVATE KEY-----\n..."
}
```

Панель выдаёт это значение при создании ноды.

`INTERNAL_REST_TOKEN`, `INTERNAL_SOCKET_PATH` и `XTLS_API_SOCKET_PATH`
генерируются при первом запуске и сохраняются в `/run/rw-node.env` с правами
`0600`, чтобы CLI и перезапущенный процесс использовали те же значения.

Готовый шаблон — в [`.env.sample`](.env.sample).

## Локальный запуск

```bash
export NODE_PORT=8443
export SECRET_KEY='<base64 JSON payload>'
export RUST_LOG=info

cargo run --release -p rw-node
```

Процессу нужен доступ на запись в `/run` для runtime env-файла. Для nftables и
установки ядра Xray также требуются повышенные привилегии.

## CLI

CLI доступен как `rw-node cli <команда>` или через симлинк `cli` в контейнере.

```bash
cargo run --release -p rw-node -- cli --help
cargo run --release -p rw-node -- cli --dump-config
cargo run --release -p rw-node -- cli --dump-config-raw
cargo run --release -p rw-node -- cli --kill-sockets 203.0.113.10
```

## Docker

Образ собирается через BuildKit. Рекомендуется buildx с cache mounts:

```bash
docker buildx build -f docker/Dockerfile -t remnawave/node:rust --load .
```

Для multi-platform сборки используйте `docker-bake.hcl`; нужен драйвер
`docker-container` и эмуляция QEMU/binfmt:

```bash
docker buildx bake -f docker-bake.hcl
```

Пример запуска:

```bash
docker run -d \
  --name remnanode \
  --network host \
  --restart always \
  --cap-add NET_ADMIN \
  --device /dev/net/tun \
  --ulimit nofile=1048576:1048576 \
  --env-file .env \
  -e NODE_PORT=2222 \
  remnawave/node:rust
```

## Debian-пакет

В репозитории есть сборочный каталог `debian/`. Пакет ставит `rw-node` вместе
с ядром Xray, геоданными, базой ASN и geocheck. При сборке эти артефакты
скачиваются с GitHub, поэтому нужен доступ в сеть.

```bash
sudo apt install debhelper build-essential cargo rustc cmake curl perl unzip zstd
dpkg-buildpackage -us -uc -b
sudo apt install ./rw-node_*.deb
sudoedit /etc/rw-node/env
sudo systemctl enable --now rw-node
```

Если `cargo`/`rustc` установлены через rustup, а не через apt, передайте `-d`
в `dpkg-buildpackage`, чтобы пропустить проверку build-dependencies.

Systemd-юнит читает `/etc/rw-node/env`. Runtime-токены и имена абстрактных
сокетов по-прежнему генерируются в `/run/rw-node.env`. При установке сервис не
включается и не запускается автоматически.

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

```bash
cargo check --workspace
cargo test --workspace --lib --bins
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

## Структура проекта

```
crates/
├── rw-node/         # бинарник, CLI, instance-lock, runtime env
├── http/            # axum-роутеры, mTLS, JWT, внутренний API
├── contract/        # маршруты, коды ошибок, DTO, константы Xray
├── config/          # декодирование SECRET_KEY, PEM, SNI, env
├── core/            # системная/сетевая статистика, загрузки, webhook
├── xray-rpc/        # tonic-клиент и вендоренные protobuf-файлы
├── xray/            # супервизор Xray, core loader, geodata, service
├── hashed-set/      # точный порт @remnawave/hashed-set
├── node-plugins/    # состояние плагинов, nftables, torrent blocker
└── asn-lmdb/        # read-only поиск по ASN LMDB
```

## Лицензия

AGPL-3.0-only. См. [`LICENCE`](LICENCE).
