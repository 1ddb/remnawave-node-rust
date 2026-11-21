#!/bin/sh
set -eu

dest="${1:?usage: $0 <destination-root>}"

XRAY_CORE_VERSION="${XRAY_CORE_VERSION:-v26.11.9}"
UPSTREAM_REPO="${UPSTREAM_REPO:-XTLS}"
GEOCHECK_VERSION="${GEOCHECK_VERSION:-0.3.0}"
ASN_LMDB_URL="${ASN_LMDB_URL:-https://github.com/remnawave/asn-index/releases/latest/download/asn-prefixes.lmdb.zst}"

arch="${DEB_HOST_ARCH:-$(dpkg --print-architecture)}"
case "$arch" in
    amd64)
        xray_archive="Xray-linux-64.zip"
        geocheck_archive="geocheck_linux_amd64.tar.gz"
        ;;
    arm64)
        xray_archive="Xray-linux-arm64-v8a.zip"
        geocheck_archive="geocheck_linux_arm64.tar.gz"
        ;;
    *)
        echo "fetch-assets: unsupported architecture: $arch" >&2
        exit 1
        ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

xray_base="https://github.com/${UPSTREAM_REPO}/Xray-core/releases/download/${XRAY_CORE_VERSION}"
curl -fsSL --retry 3 -o "$tmp/$xray_archive" "$xray_base/$xray_archive"
curl -fsSL --retry 3 -o "$tmp/$xray_archive.dgst" "$xray_base/$xray_archive.dgst"
xray_sha="$(awk '/^SHA2-256=/ { print $2 }' "$tmp/$xray_archive.dgst")"
if [ -z "$xray_sha" ]; then
    echo "fetch-assets: Xray checksum is missing" >&2
    exit 1
fi
printf '%s  %s\n' "$xray_sha" "$tmp/$xray_archive" | sha256sum -c -

mkdir -p "$tmp/xray"
unzip -q "$tmp/$xray_archive" -d "$tmp/xray"
install -Dm755 "$tmp/xray/xray" "$dest/usr/local/bin/xray"
install -Dm644 "$tmp/xray/geoip.dat" "$dest/usr/local/share/xray/geoip.dat"
install -Dm644 "$tmp/xray/geosite.dat" "$dest/usr/local/share/xray/geosite.dat"

curl -fsSL --retry 3 -o "$tmp/asn-prefixes.lmdb.zst" "$ASN_LMDB_URL"
install -d "$dest/usr/local/share/asn"
zstd -q -d -f "$tmp/asn-prefixes.lmdb.zst" -o "$dest/usr/local/share/asn/asn-prefixes.lmdb"
chmod 644 "$dest/usr/local/share/asn/asn-prefixes.lmdb"

geocheck_base="https://github.com/remnawave/geocheck/releases/download/v${GEOCHECK_VERSION}"
curl -fsSL --retry 3 -o "$tmp/$geocheck_archive" "$geocheck_base/$geocheck_archive"
curl -fsSL --retry 3 -o "$tmp/checksums.txt" "$geocheck_base/checksums.txt"
geocheck_sha="$(awk -v file="$geocheck_archive" '$2 == file { print $1 }' "$tmp/checksums.txt")"
if [ -z "$geocheck_sha" ]; then
    echo "fetch-assets: geocheck checksum is missing" >&2
    exit 1
fi
printf '%s  %s\n' "$geocheck_sha" "$tmp/$geocheck_archive" | sha256sum -c -
mkdir -p "$tmp/geocheck"
tar -xzf "$tmp/$geocheck_archive" -C "$tmp/geocheck" geocheck
install -Dm755 "$tmp/geocheck/geocheck" "$dest/usr/local/bin/geocheck"
