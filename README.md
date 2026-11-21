# Remnawave Node (Rust)

Rust reimplementation of [`remnawave/node`](https://github.com/remnawave/node),
API-compatible with `node` 3.4.3. This is the agent that runs on every node and
is controlled by the Remnawave Panel.

## Node.js vs Rust

A drop-in reimplementation of the upstream Node.js agent:

| Metric | Node.js (upstream) | Rust (this project) |
|---|---|---|
| Language | TypeScript / Node.js | Rust |
| Memory | ~107 MiB | ~6 MiB |
| Image, disk | 510 MB | 177 MB |
| Image, content | 124 MB | 87.9 MB |
| CPU idle | 0.0% | 0.0% |

The Rust agent uses roughly **17x less resident memory** than the Node.js one,
and the Docker image is about **2.9x smaller on disk** and **1.4x smaller in
content size**.

## Features

- Public HTTPS API with mutual TLS, optional SNI verification and JWT (RS256)
  authentication on every route.
- Internal HTTP API on an abstract unix socket with token authentication.
- Xray Core supervisor: process spawn, crash detection, exponential backoff
  (1–30 s) and log capture in `/var/log/xray/current`.
- gRPC client for the Xray API over an abstract unix socket.
- User handlers, online/traffic statistics, geocheck, plugins and nftables.
- Custom Xray core and geodata assets with SHA-256 verification and atomic
  installation.
- Built-in CLI for config dumps and socket management.

## Requirements

- Linux with abstract unix socket and nftables support.
- Rust 1.85 or newer.
- Runtime privileges:
  - `NET_ADMIN` for nftables;
  - `/dev/net/tun` for TUN inbounds;
  - write access to `/usr/local/bin`, `/var/log/xray`, `/run` and
    `/usr/local/share`.

## Configuration

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `NODE_PORT` | yes | — | HTTPS API port. |
| `SECRET_KEY` | yes | — | Base64-encoded JSON payload from the Panel. |
| `SNI_VERIFICATION` | no | `false` | Enforce the derived SNI on TLS handshakes. |
| `DISABLE_HASHED_SET_CHECK` | no | `false` | Disable the hashed-set config change check. |
| `NFTABLES_LOGGING` | no | `true` | Enable nftables rule logging. |
| `NFTABLES_ACCEPT_REPLY_TRAFFIC` | no | `false` | Accept reply traffic in nftables rules. |
| `XRAY_LOCATION_ASSET` | no | `/usr/local/share/xray` | Directory for Xray geodata assets. |
| `RUST_LOG` | no | `info` | `tracing` filter (`debug` when `NODE_ENV=development`). |
| `NODE_ENV` | no | `production` | Set to `development` for debug logs. |
| `INTERNAL_REST_TOKEN` | no | generated | Token for the internal API. |
| `INTERNAL_SOCKET_PATH` | no | generated | Abstract unix socket for the internal API. |
| `XTLS_API_SOCKET_PATH` | no | generated | Abstract unix socket for the Xray gRPC API. |

`SECRET_KEY` is a base64-encoded JSON object with the following fields:

```json
{
  "caCertPem": "-----BEGIN CERTIFICATE-----\n...",
  "jwtPublicKey": "-----BEGIN PUBLIC KEY-----\n...",
  "nodeCertPem": "-----BEGIN CERTIFICATE-----\n...",
  "nodeKeyPem": "-----BEGIN PRIVATE KEY-----\n..."
}
```

The Panel provides this value when a node is created.

`INTERNAL_REST_TOKEN`, `INTERNAL_SOCKET_PATH` and `XTLS_API_SOCKET_PATH` are
generated on first start and persisted in `/run/rw-node.env` with mode `0600`,
so the CLI and restarted instances keep using the same values.

See [`.env.sample`](.env.sample) for a ready-to-copy template.

## Running locally

```bash
export NODE_PORT=8443
export SECRET_KEY='<base64 JSON payload>'
export RUST_LOG=info

cargo run --release -p rw-node
```

The process needs write access to `/run` for the runtime env file. nftables and
the Xray core installation also require elevated privileges.

## CLI

The CLI is available as `rw-node cli <command>` or through the `cli` symlink in
the container.

```bash
cargo run --release -p rw-node -- cli --help
cargo run --release -p rw-node -- cli --dump-config
cargo run --release -p rw-node -- cli --dump-config-raw
cargo run --release -p rw-node -- cli --kill-sockets 203.0.113.10
```

## Docker

The image is built with BuildKit. Buildx with cache mounts is recommended:

```bash
docker buildx build -f docker/Dockerfile -t remnawave/node:rust --load .
```

For multi-platform builds use `docker-bake.hcl`; it requires a
`docker-container` builder and QEMU/binfmt emulation:

```bash
docker buildx bake -f docker-bake.hcl
```

Example run:

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

## Debian package

A `debian/` source package is included. It builds `rw-node` together with the
bundled Xray core, geodata, ASN database and geocheck. The build downloads those
artifacts from GitHub, so network access is required.

```bash
sudo apt install debhelper build-essential cargo rustc cmake curl perl unzip zstd
dpkg-buildpackage -us -uc -b
sudo apt install ./rw-node_*.deb
sudoedit /etc/rw-node/env
sudo systemctl enable --now rw-node
```

If `cargo`/`rustc` are installed through rustup rather than apt, pass `-d` to
`dpkg-buildpackage` to skip the build-dependency check.

The systemd unit reads `/etc/rw-node/env`. Runtime tokens and abstract socket
names are still generated into `/run/rw-node.env`. The service is not enabled or
started automatically during installation.

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

```bash
cargo check --workspace
cargo test --workspace --lib --bins
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

## Project layout

```
crates/
├── rw-node/         # binary, CLI, instance lock, runtime env
├── http/            # axum routers, mTLS, JWT, internal API
├── contract/        # routes, error codes, DTOs, Xray constants
├── config/          # SECRET_KEY decoding, PEM, SNI, env
├── core/            # system/network stats, downloads, webhooks
├── xray-rpc/        # tonic client and vendored protobufs
├── xray/            # Xray supervisor, core loader, geodata, service
├── hashed-set/      # exact port of @remnawave/hashed-set
├── node-plugins/    # plugin state, nftables, torrent blocker
└── asn-lmdb/        # read-only ASN LMDB lookup
```

## License

AGPL-3.0-only. See [`LICENCE`](LICENCE).
