# Remnawave Node on Rust

[English](README.md) | [Русский](README.ru.md)

Rust reimplementation of [`remnawave/node`](https://github.com/remnawave/node),
API-compatible with `node` 3.4.6 (upstream commit `52d0f1c`). Drop-in
replacement for the agent controlled by the Remnawave Panel.

## Node.js vs Rust

| Metric | [remnawave/node](https://github.com/remnawave/node) | 1ddb/remnawave-node-rust |
|---|---|---|
| Language | TypeScript / Node.js | Rust |
| Memory | ~107 MiB | ~6 MiB |
| Image, disk | 510 MB | 177 MB |
| Image, content | 124 MB | 87.9 MB |
| CPU idle | 0.0% | 0.0% |
| xray-core | separate binary | separate binary |

*Comparison excludes xray-core.*

The Rust agent uses roughly **17x less resident memory** than the Node.js one,
and the Docker image is about **2.9x smaller on disk** and **1.4x smaller in
content size**.

## Features

- Public HTTPS API with mutual TLS, optional SNI verification and JWT (RS256)
  authentication on every route.
- Internal HTTP API on an abstract unix socket with token authentication.
- Xray Core supervisor: process spawn, crash detection, exponential backoff
  (1–30 s) and log capture in `/var/log/xray/current`.
- gRPC client for the Xray API, user handlers, statistics, geocheck, plugins
  and nftables.
- Custom Xray core and geodata assets with SHA-256 verification and atomic
  installation.
- Built-in CLI for config dumps and socket management.

## Requirements

- Linux with nftables and abstract unix sockets.
- Debian 12+ or Ubuntu 22.04+ for the `.deb` package.
- `NET_ADMIN` for nftables and `/dev/net/tun` for TUN inbounds.
- Write access to `/usr/local/bin`, `/var/log/xray`, `/run` and
  `/usr/local/share`.

## Migration

The agent is a drop-in replacement: keep the existing `.env` (`NODE_PORT`,
`SECRET_KEY` and optional variables).

### Docker Compose

Replace the `image` in `docker-compose-prod.yml`:

```yaml
services:
  remnanode:
    image: ghcr.io/1ddb/remnawave-node-rust:latest
```

```bash
docker compose pull
docker compose up -d
```

For a new installation, copy [`docker-compose-prod.yml`](docker-compose-prod.yml) and
[`.env.sample`](.env.sample) first; `SECRET_KEY` is available in the Panel under
the node settings or when creating the node.

### Debian package (optional)

```bash
curl -fsSLO "https://github.com/1ddb/remnawave-node-rust/releases/download/v3.4.6/rw-node_3.4.6-1_$(dpkg --print-architecture).deb"
sudo apt install "./rw-node_3.4.6-1_$(dpkg --print-architecture).deb"
sudoedit /etc/rw-node/env   # paste SECRET_KEY from the Panel
sudo docker stop remnanode || true   # free the port if the old container exists
sudo systemctl enable --now rw-node
```

Migrating from Docker? Run `docker compose down` in the old installation
directory to remove the stopped container.

The package bundles Xray Core, geodata, the ASN database and geocheck.

## Configuration

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `NODE_PORT` | yes | — | HTTPS API port. |
| `SECRET_KEY` | yes | — | Base64-encoded JSON with certificates and the JWT public key from the Panel. |
| `SNI_VERIFICATION` | no | `false` | Enforce the derived SNI on TLS handshakes. |
| `DISABLE_HASHED_SET_CHECK` | no | `false` | Disable the hashed-set config change check. |
| `NFTABLES_LOGGING` | no | `true` | Enable nftables rule logging. |
| `NFTABLES_ACCEPT_REPLY_TRAFFIC` | no | `false` | Accept reply traffic in nftables rules. |
| `NFTABLES_TABLE_NAME` | no | `remnanode` | nftables table name; IPv6 uses a `6` suffix. |
| `SUPPRESS_DUPLICATE_INSTANCE_WARNING` | no | `false` | Suppress the warning about another node in the same network namespace. |
| `XRAY_LOCATION_ASSET` | no | `/usr/local/share/xray` | Directory for Xray geodata assets. |
| `RUST_LOG` | no | `info` | `tracing` filter (`debug` when `NODE_ENV=development`). |
| `NODE_ENV` | no | `production` | Set to `development` for debug logs. |
| `INTERNAL_REST_TOKEN` | no | generated | Token for the internal API. |
| `INTERNAL_SOCKET_PATH` | no | generated | Abstract unix socket for the internal API. |
| `XTLS_API_SOCKET_PATH` | no | generated | Abstract unix socket for the Xray gRPC API. |

`INTERNAL_REST_TOKEN`, `INTERNAL_SOCKET_PATH` and `XTLS_API_SOCKET_PATH` are
generated on first start and persisted in `/run/rw-node.env` with mode `0600`,
so the CLI and restarted instances keep using the same values.

See [`.env.sample`](.env.sample) for a ready-to-copy template.

## CLI

The CLI is available as `rw-node cli <command>` or through the `cli` symlink in
the container. For the `.deb` install, load the service environment first:
`source /etc/rw-node/env`.

```bash
rw-node cli --help
rw-node cli --dump-config
rw-node cli --dump-config-raw
rw-node cli --kill-sockets 203.0.113.10
```

## Runtime files

| Path | Purpose |
| --- | --- |
| `/usr/local/bin/rw-core` | Symlink to the active Xray core. |
| `/usr/local/bin/xray` | Bundled Xray core. |
| `/usr/local/bin/xray-custom` | Installed custom Xray core. |
| `/usr/local/bin/.rw-core.json` | Metadata of the installed custom core. |
| `/var/log/xray/current` | Xray process output. |
| `/run/rw-node.env` | Generated internal tokens and socket names. |
| `/usr/local/share/xray` | `geoip.dat` and `geosite.dat`. |
| `/usr/local/share/asn/asn-prefixes.lmdb` | ASN prefix database. |

## Development

Build instructions and project layout: [`DEVELOPMENT.md`](DEVELOPMENT.md).

## License

AGPL-3.0-only. See [`LICENCE`](LICENCE).
