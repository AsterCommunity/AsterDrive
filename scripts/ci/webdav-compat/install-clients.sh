#!/usr/bin/env bash

set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source-path=SCRIPTDIR
# shellcheck source=versions.env
source "$script_dir/versions.env"

install_prefix=${WEBDAV_COMPAT_TOOLS_DIR:-"$HOME/.local/webdav-compat"}
bin_dir="$install_prefix/bin"
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/asterdrive-webdav-tools.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT

if [[ $(uname -s) != Linux ]]; then
  echo "This CI installer supports Linux only." >&2
  exit 1
fi

case $(uname -m) in
  x86_64)
    rclone_arch=amd64
    rclone_sha256=$RCLONE_LINUX_AMD64_SHA256
    ;;
  aarch64 | arm64)
    rclone_arch=arm64
    rclone_sha256=$RCLONE_LINUX_ARM64_SHA256
    ;;
  *)
    echo "Unsupported Linux architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

mkdir -p "$bin_dir"

download_and_verify() {
  local url=$1
  local destination=$2
  local expected_sha256=$3

  curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
    "$url" --output "$destination"
  printf '%s  %s\n' "$expected_sha256" "$destination" | sha256sum --check --status
}

install_rclone() {
  local archive="$work_dir/rclone.zip"
  local source_dir="$work_dir/rclone-v${RCLONE_VERSION}-linux-${rclone_arch}"
  download_and_verify \
    "https://downloads.rclone.org/v${RCLONE_VERSION}/rclone-v${RCLONE_VERSION}-linux-${rclone_arch}.zip" \
    "$archive" \
    "$rclone_sha256"
  unzip -q "$archive" -d "$work_dir"
  install -m 0755 "$source_dir/rclone" "$bin_dir/rclone"
}

install_curl() {
  local archive="$work_dir/curl.tar.xz"
  local source_dir="$work_dir/curl-${CURL_VERSION}"
  local build_log="$work_dir/curl-build.log"
  download_and_verify \
    "https://github.com/curl/curl/releases/download/curl-${CURL_VERSION//./_}/curl-${CURL_VERSION}.tar.xz" \
    "$archive" \
    "$CURL_SOURCE_SHA256"
  tar -xJf "$archive" -C "$work_dir"
  if ! (
    cd "$source_dir"
    ./configure \
      --prefix="$install_prefix" \
      --disable-shared \
      --enable-static \
      --disable-ldap \
      --disable-ldaps \
      --disable-manual \
      --disable-docs \
      --disable-dict \
      --disable-file \
      --disable-ftp \
      --disable-gopher \
      --disable-imap \
      --disable-ipfs \
      --disable-mqtt \
      --disable-pop3 \
      --disable-rtsp \
      --disable-smtp \
      --disable-telnet \
      --disable-tftp \
      --enable-unity \
      --without-brotli \
      --without-libidn2 \
      --without-libpsl \
      --without-ssl \
      --without-zlib \
      --without-zstd
    make -j"$(nproc)"
    make install
  ) >"$build_log" 2>&1; then
    cat "$build_log" >&2
    return 1
  fi
}

install_cadaver() {
  local archive="$work_dir/cadaver.tar.gz"
  local source_dir="$work_dir/cadaver-${CADAVER_VERSION}"
  local build_log="$work_dir/cadaver-build.log"
  download_and_verify \
    "https://notroj.github.io/cadaver/cadaver-${CADAVER_VERSION}.tar.gz" \
    "$archive" \
    "$CADAVER_SOURCE_SHA256"
  tar -xzf "$archive" -C "$work_dir"
  if ! (
    cd "$source_dir"
    ./configure \
      --prefix="$install_prefix" \
      --disable-readline \
      --with-neon=/usr
    make -j"$(nproc)"
    make install
  ) >"$build_log" 2>&1; then
    cat "$build_log" >&2
    return 1
  fi
}

install_rclone
install_curl
install_cadaver

export PATH="$bin_dir:$PATH"
rclone_version=$(rclone version | sed -n '1p')
curl_version=$(curl --version | sed -n '1p')
cadaver_version=$({ cadaver --version 2>&1 || true; } | sed -n '1p')

[[ $rclone_version == "rclone v${RCLONE_VERSION}" ]]
[[ $curl_version == "curl ${CURL_VERSION} "* ]]
[[ $cadaver_version == "cadaver ${CADAVER_VERSION}" ]]

cat <<EOF
Installed pinned WebDAV client toolchain:
  $rclone_version
  $curl_version
  $cadaver_version
EOF
