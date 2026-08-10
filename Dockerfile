# Stage 1: Build frontend
FROM node:24-alpine AS frontend

RUN npm install -g bun@latest

WORKDIR /build/frontend-panel
COPY frontend-panel/package.json frontend-panel/bun.lock* ./
RUN bun install --frozen-lockfile

COPY frontend-panel/ ./
RUN bun run build

# Stage 2: Build Rust binary
FROM rust:1-alpine AS builder

RUN apk add --no-cache build-base pkgconfig sqlite-dev curl

WORKDIR /build
ARG CARGO_FEATURES="server,cli"

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY benches/ benches/
COPY tests/multi_primary/ tests/multi_primary/

# Pre-build dependencies (cache layer)
RUN mkdir src && \
    echo 'fn main() {}' > src/main.rs && \
    echo '' > src/lib.rs && \
    cargo build --release --locked --features "${CARGO_FEATURES}" && \
    rm -rf src

COPY src/ src/
COPY build.rs ./
COPY --from=frontend /build/frontend-panel/dist/ frontend-panel/dist/

ARG ASTER_BUILD_REVISION="unknown"

RUN cargo build --release --locked --features "${CARGO_FEATURES}"

# Stage 3: Shared Alpine runtime
FROM alpine:3.24 AS runtime-base

RUN apk add --no-cache ca-certificates sqlite-libs && \
    addgroup -S -g 10001 aster && \
    adduser -S -D -H -u 10001 -G aster -s /sbin/nologin aster && \
    mkdir -p /data && \
    chown -R aster:aster /data

LABEL maintainer="AptS:1547 <apts-1547@esaps.net>"
LABEL org.opencontainers.image.title="AsterDrive"
LABEL org.opencontainers.image.description="Self-hosted cloud storage system built with Rust"
LABEL org.opencontainers.image.source="https://github.com/AsterCommunity/AsterDrive"
LABEL org.opencontainers.image.license="MIT"

VOLUME ["/data"]
EXPOSE 3000

WORKDIR /
ENV ASTER__SERVER__HOST=0.0.0.0

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD ["wget", "-q", "-O", "/dev/null", "http://127.0.0.1:3000/health/ready"]

ENTRYPOINT ["/usr/local/bin/aster_drive"]

# Stage 4: Slim runtime base without optional external media processors
FROM runtime-base AS runtime-slim-base

ENV ASTER_BOOTSTRAP_ENABLE_VIPS_CLI=false
ENV ASTER_BOOTSTRAP_ENABLE_FFMPEG_CLI=false
ENV ASTER_BOOTSTRAP_ENABLE_FFPROBE_CLI=false

# Stage 5: Slim runtime image
FROM runtime-slim-base AS runtime-slim

COPY --from=builder /build/target/release/aster_drive /usr/local/bin/aster_drive

USER aster:aster

# Stage 6: Full runtime base with the optional external media processors
# Alpine 3.24 still ships libtiff 4.7.1, which is affected by CVE-2026-4775.
FROM runtime-base AS runtime-full-base

RUN apk add --no-cache vips-tools vips-poppler 'ffmpeg>=8.1.2-r0' libheif && \
    apk add --no-cache \
      --repository=https://dl-cdn.alpinelinux.org/alpine/edge/main \
      'tiff=4.7.2-r0'

ENV ASTER_BOOTSTRAP_ENABLE_VIPS_CLI=true
ENV ASTER_BOOTSTRAP_ENABLE_FFMPEG_CLI=true
ENV ASTER_BOOTSTRAP_ENABLE_FFPROBE_CLI=true

# Stage 7: Full runtime image (kept last so plain `docker build` remains full)
FROM runtime-full-base AS runtime-full

COPY --from=builder /build/target/release/aster_drive /usr/local/bin/aster_drive

USER aster:aster
