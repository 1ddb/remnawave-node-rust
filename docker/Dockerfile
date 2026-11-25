# syntax=docker/dockerfile:1.7

FROM rust:1.91-alpine AS build

RUN apk add --no-cache musl-dev build-base

ARG TARGETARCH

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

ENV CARGO_INCREMENTAL=0

RUN --mount=type=cache,id=cargo-registry-${TARGETARCH},target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=cargo-git-${TARGETARCH},target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=cargo-target-${TARGETARCH},target=/cargo-target,sharing=locked \
    CARGO_TARGET_DIR=/cargo-target \
    cargo build --release --locked --bin rw-node \
    && install -m 755 /cargo-target/release/rw-node /usr/local/bin/rw-node


FROM alpine:3.21 AS xray

ARG XRAY_CORE_VERSION=v26.11.9
ARG UPSTREAM_REPO=XTLS
ARG TARGETARCH

RUN apk add --no-cache curl unzip \
    && case "${TARGETARCH}" in \
        amd64) ARCHIVE='Xray-linux-64.zip' ;; \
        arm64) ARCHIVE='Xray-linux-arm64-v8a.zip' ;; \
    esac \
    && BASE="https://github.com/${UPSTREAM_REPO}/Xray-core/releases/download/${XRAY_CORE_VERSION}" \
    && curl -fsSL -o "/tmp/${ARCHIVE}" "${BASE}/${ARCHIVE}" \
    && curl -fsSL -o /tmp/xray.dgst "${BASE}/${ARCHIVE}.dgst" \
    && echo "$(awk '/^SHA2-256=/ { print $2 }' /tmp/xray.dgst)  /tmp/${ARCHIVE}" | sha256sum -c - \
    && unzip -q "/tmp/${ARCHIVE}" -d /tmp/xray \
    && install -m 755 /tmp/xray/xray /usr/local/bin/xray \
    && install -d /usr/local/share/xray \
    && install -m 644 /tmp/xray/geoip.dat /usr/local/share/xray/geoip.dat \
    && install -m 644 /tmp/xray/geosite.dat /usr/local/share/xray/geosite.dat \
    && rm -rf "/tmp/${ARCHIVE}" /tmp/xray /tmp/xray.dgst


FROM alpine:3.21 AS asn

ARG ASN_LMDB_URL=https://github.com/remnawave/asn-index/releases/latest/download/asn-prefixes.lmdb.zst

RUN apk add --no-cache zstd

ADD ${ASN_LMDB_URL} /tmp/asn-prefixes.lmdb.zst

RUN mkdir -p /usr/local/share/asn \
    && zstd -d /tmp/asn-prefixes.lmdb.zst -o /usr/local/share/asn/asn-prefixes.lmdb \
    && chmod 644 /usr/local/share/asn/asn-prefixes.lmdb \
    && rm -f /tmp/asn-prefixes.lmdb.zst


FROM alpine:3.21 AS geocheck

ARG GEOCHECK_VERSION=0.3.0
ARG GEOCHECK_RELEASE_URL=https://github.com/remnawave/geocheck/releases/download
ARG TARGETARCH

RUN apk add --no-cache curl \
    && cd /tmp \
    && ARCHIVE="geocheck_linux_${TARGETARCH}.tar.gz" \
    && curl -fsSL -O "${GEOCHECK_RELEASE_URL}/v${GEOCHECK_VERSION}/${ARCHIVE}" \
    && curl -fsSL -O "${GEOCHECK_RELEASE_URL}/v${GEOCHECK_VERSION}/checksums.txt" \
    && grep "  ${ARCHIVE}\$" checksums.txt | sha256sum -c - \
    && tar -xzf "${ARCHIVE}" geocheck \
    && install -m 0755 geocheck /usr/local/bin/geocheck \
    && rm -rf /tmp/*


FROM alpine:3.21

LABEL org.opencontainers.image.title="Remnawave Node"
LABEL org.opencontainers.image.description="Remnawave Node (Rust) with built-in XRay Core"
LABEL org.opencontainers.image.url="https://github.com/1ddb/remnawave-node-rust"
LABEL org.opencontainers.image.source="https://github.com/1ddb/remnawave-node-rust"
LABEL org.opencontainers.image.vendor="Remnawave"
LABEL org.opencontainers.image.licenses="AGPL-3.0-only"
LABEL org.opencontainers.image.documentation="https://docs.rw"

RUN apk add --no-cache ca-certificates iproute2 \
    && mkdir -p /var/log/xray /run \
    && ln -s /usr/local/bin/xray /usr/local/bin/rw-core \
    && ln -s /usr/local/bin/rw-node /usr/local/bin/cli \
    && printf '#!/bin/sh\ntail -n +1 -f /var/log/xray/current\n' > /usr/local/bin/xlogs \
    && printf '#!/bin/sh\ntail -n +1 -f /var/log/xray/current\n' > /usr/local/bin/xerrors \
    && chmod +x /usr/local/bin/xlogs /usr/local/bin/xerrors

COPY --from=build /usr/local/bin/rw-node /usr/local/bin/rw-node
COPY --from=xray /usr/local/bin/xray /usr/local/bin/xray
COPY --from=xray /usr/local/share/xray/geoip.dat /usr/local/share/xray/geoip.dat
COPY --from=xray /usr/local/share/xray/geosite.dat /usr/local/share/xray/geosite.dat
COPY --from=asn /usr/local/share/asn /usr/local/share/asn
COPY --from=geocheck /usr/local/bin/geocheck /usr/local/bin/geocheck

ENV NODE_ENV=production
ENV XRAY_JSON_STRICT=true

ENTRYPOINT ["/usr/local/bin/rw-node"]
