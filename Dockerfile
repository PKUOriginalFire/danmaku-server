FROM rust:alpine AS builder
RUN apk add build-base
COPY . /app
WORKDIR /app
RUN --mount=type=cache,target=/usr/local/cargo,from=rust:alpine,source=/usr/local/cargo \
    --mount=type=cache,target=target \
    cargo build --release && cp target/release/danmaku-server .

FROM alpine
COPY --from=builder /app/danmaku-server /usr/local/bin
ENTRYPOINT ["danmaku-server"]
