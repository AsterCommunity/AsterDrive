# Official distributions are unavailable; build the immutable upstream release source.
FROM golang:1.24.2-bookworm@sha256:79390b5e5af9ee6e7b1173ee3eac7fadf6751a545297672916b59bfa0ecf6f71 AS build
ADD --checksum=sha256:8819e3e7817e46b7b3798f8f200ead208562e571563c2e040352378031abe9f2 https://codeload.github.com/minio/minio/tar.gz/07c3a429bfed433e49018cb0f78a52145d4bedeb /tmp/source.tar.gz
WORKDIR /src
RUN tar -xzf /tmp/source.tar.gz --strip-components=1
RUN CGO_ENABLED=0 GOMAXPROCS=2 go build -p=1 -trimpath -ldflags="-s -w" -o /minio .

FROM scratch
COPY --from=build /minio /minio
ENTRYPOINT ["/minio"]
