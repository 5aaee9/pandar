FROM rust:1-bookworm AS builder

WORKDIR /app
COPY . .
RUN cargo build --release --bin pandar-hub

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/pandar-hub /usr/local/bin/pandar-hub

ENV PANDAR_HUB_BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["pandar-hub"]
