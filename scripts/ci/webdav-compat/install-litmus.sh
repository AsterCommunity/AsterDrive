#!/usr/bin/env bash

set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source-path=SCRIPTDIR
# shellcheck source=versions.env
source "$script_dir/versions.env"

install_prefix=${WEBDAV_COMPAT_TOOLS_DIR:-"$HOME/.local/webdav-compat"}
bin_dir="$install_prefix/bin"
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/asterdrive-litmus.XXXXXX")
build_log="$work_dir/litmus-build.log"
trap 'rm -rf "$work_dir"' EXIT

case $(uname -s) in
  Linux | Darwin) ;;
  *)
    echo "Unsupported operating system: $(uname -s)" >&2
    exit 1
    ;;
esac

for command in aclocal autoconf autoheader curl make pkg-config tar; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required build command is missing: $command" >&2
    exit 1
  fi
done

verify_sha256() {
  local file=$1
  local expected=$2
  local actual

  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$file" | awk '{print $1}')
  else
    actual=$(shasum -a 256 "$file" | awk '{print $1}')
  fi

  if [[ $actual != "$expected" ]]; then
    echo "SHA-256 mismatch for $file: expected $expected, got $actual" >&2
    exit 1
  fi
}

download_and_verify() {
  local url=$1
  local destination=$2
  local expected_sha256=$3

  curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
    "$url" --output "$destination"
  verify_sha256 "$destination" "$expected_sha256"
}

litmus_archive="$work_dir/litmus.tar.gz"
neon_archive="$work_dir/neon.tar.gz"
source_dir="$work_dir/litmus"

download_and_verify \
  "https://codeload.github.com/notroj/litmus/tar.gz/${LITMUS_SOURCE_COMMIT}" \
  "$litmus_archive" \
  "$LITMUS_SOURCE_SHA256"
download_and_verify \
  "https://codeload.github.com/notroj/neon/tar.gz/${LITMUS_NEON_COMMIT}" \
  "$neon_archive" \
  "$LITMUS_NEON_SOURCE_SHA256"

mkdir -p "$source_dir"
tar -xzf "$litmus_archive" --strip-components=1 -C "$source_dir"
rm -rf "$source_dir/neon"
mkdir -p "$source_dir/neon"
tar -xzf "$neon_archive" --strip-components=1 -C "$source_dir/neon"

jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)
if [[ -z $jobs ]] && command -v sysctl >/dev/null 2>&1; then
  jobs=$(sysctl -n hw.ncpu)
fi
jobs=${jobs:-2}

if [[ $(uname -s) == Darwin ]] && command -v brew >/dev/null 2>&1; then
  openssl_prefix=$(brew --prefix openssl@3)
  export PKG_CONFIG_PATH="$openssl_prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  export CPPFLAGS="-I$openssl_prefix/include${CPPFLAGS:+ $CPPFLAGS}"
  export LDFLAGS="-L$openssl_prefix/lib${LDFLAGS:+ $LDFLAGS}"
fi

if ! (
  cd "$source_dir"
  ./autogen.sh
  ./configure \
    --prefix="$install_prefix" \
    --with-included-neon \
    --with-ssl=openssl
  make -j"$jobs"
  make install
) >"$build_log" 2>&1; then
  cat "$build_log" >&2
  exit 1
fi

version=$($bin_dir/litmus --version)
if [[ $version != "litmus ${LITMUS_VERSION}" ]]; then
  echo "Unexpected Litmus version: $version" >&2
  exit 1
fi

for suite in \
  basic copymove props locks http \
  largefile lockbomb lockbomb-single protected; do
  if [[ ! -x "$install_prefix/libexec/litmus/$suite" ]]; then
    echo "Installed Litmus suite is missing: $suite" >&2
    exit 1
  fi
done

cat <<EOF_VERSION
Installed pinned WebDAV conformance tool:
  $version
  litmus commit: $LITMUS_SOURCE_COMMIT
  neon commit:   $LITMUS_NEON_COMMIT
  prefix:        $install_prefix
EOF_VERSION
