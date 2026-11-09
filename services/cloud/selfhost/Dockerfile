# Built from the root of the source package, see docker-compose.yml.
FROM rust:1-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p tronic-cloud --bin tronic-cloud

FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --home-dir /data --create-home --shell /usr/sbin/nologin troniccad \
    && mkdir -p /data/store && chown -R troniccad:troniccad /data
COPY --from=build /src/target/release/tronic-cloud /usr/local/bin/tronic-cloud
USER troniccad
ENV TRONIC_CLOUD_BIND=0.0.0.0:8787 TRONIC_STORE_DIR=/data/store
EXPOSE 8787
CMD ["tronic-cloud"]
