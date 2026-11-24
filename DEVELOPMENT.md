# Development

## Requirements

- Linux with abstract unix socket and nftables support.
- Rust 1.88 or newer.
- Docker with BuildKit/buildx for image builds.
- Debian build dependencies for `.deb` builds.

Runtime privileges:

- `NET_ADMIN` for nftables;
- `/dev/net/tun` for TUN inbounds;
- write access to `/usr/local/bin`, `/var/log/xray`, `/run` and
  `/usr/local/share`.

## Local build and run

```bash
cargo build --workspace --release

export NODE_PORT=8443
export SECRET_KEY='<base64 JSON payload>'
export RUST_LOG=info
cargo run --release -p rw-node
```

The process needs write access to `/run` for the runtime env file. nftables and
the Xray core installation also require elevated privileges.

## SECRET_KEY payload

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

## Checks

```bash
cargo check --workspace
cargo test --workspace --lib --bins
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

Install missing components:

```bash
rustup component add rustfmt clippy
cargo install cargo-deny --locked
```

## Docker image

BuildKit with cache mounts is required; the legacy builder ignores them and
rebuilds dependencies from scratch:

```bash
docker buildx build -f docker/Dockerfile -t remnawave/node:rust --load .
```

For multi-platform builds use `docker-bake.hcl`; it requires a
`docker-container` builder and QEMU/binfmt emulation:

```bash
docker buildx bake -f docker-bake.hcl
```

## Debian package

```bash
sudo apt install debhelper build-essential cargo rustc cmake curl perl unzip zstd
dpkg-buildpackage -us -uc -b
sudo apt install ./rw-node_*.deb
sudoedit /etc/rw-node/env   # paste SECRET_KEY from the Panel
sudo systemctl enable --now rw-node
```

The package bundles the Xray Core binary, geodata, ASN database and geocheck;
the build downloads them from GitHub, so network access is required.

If `cargo`/`rustc` are installed through rustup rather than apt, pass `-d` to
`dpkg-buildpackage` to skip the build-dependency check.

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
