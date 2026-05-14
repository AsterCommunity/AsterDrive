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
COPY Cargo.toml Cargo.lock ./
COPY migration/ migration/
COPY api-docs-macros/ api-docs-macros/

# Pre-build dependencies (cache layer)
RUN mkdir src && echo 'fn main() {}' > src/main.rs && \
    cargo build --release 2>/dev/null || true && \
    rm -rf src

COPY src/ src/
COPY build.rs ./
COPY --from=frontend /build/frontend-panel/dist/ frontend-panel/dist/

ARG CARGO_FEATURES="server,cli"
ENV RUSTFLAGS="-C link-arg=-s"

RUN cargo build --release --features "${CARGO_FEATURES}"

# Stage 3: Alpine runtime
FROM alpine:3.23

RUN apk add --no-cache ca-certificates sqlite-libs vips-tools vips-poppler ffmpeg libheif && \
    addgroup -S -g 10001 aster && \
    adduser -S -D -H -u 10001 -G aster -s /sbin/nologin aster && \
    mkdir -p /data && \
    chown -R aster:aster /data

LABEL maintainer="AptS:1547 <apts-1547@esaps.net>"
LABEL org.opencontainers.image.title="AsterDrive"
LABEL org.opencontainers.image.description="Self-hosted cloud storage system built with Rust"
LABEL org.opencontainers.image.source="https://github.com/AptS-1547/AsterDrive"
LABEL org.opencontainers.image.license="MIT"

COPY --from=builder /build/target/release/aster_drive /usr/local/bin/aster_drive

VOLUME ["/data"]
EXPOSE 3000

WORKDIR /
ENV ASTER__SERVER__HOST=0.0.0.0
ENV ASTER_BOOTSTRAP_ENABLE_VIPS_CLI=true
ENV ASTER_BOOTSTRAP_ENABLE_FFMPEG_CLI=true


HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD ["wget", "-q", "-O", "/dev/null", "http://127.0.0.1:3000/health/ready"]

USER aster:aster

ENTRYPOINT ["/usr/local/bin/aster_drive"]
