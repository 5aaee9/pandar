FROM rust:1-bookworm@sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa AS builder

WORKDIR /app
COPY . .
RUN cargo build --release --bin pandar-hub

FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && groupadd --gid 10001 pandar \
    && useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin pandar \
    && mkdir -p /var/lib/pandar /data /spool \
    && chown -R 10001:10001 /var/lib/pandar /data /spool \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/pandar-hub /usr/local/bin/pandar-hub

WORKDIR /var/lib/pandar
USER 10001:10001
ENV PANDAR_HUB_BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["pandar-hub"]
