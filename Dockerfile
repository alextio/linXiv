# Headless linXiv node: `linxiv-headless` (full /api/* over HTTP incl. share
# routes + the iroh peer + background sync) with `linxiv-cli` alongside for
# exec-style queries. Both live in the Tauri-free linxiv-server/cli crates,
# so no webkit/gtk anywhere in this image.
#
# The image binds 0.0.0.0, so the bin fails closed: LINXIV_API_TOKEN must be
# set and every request but `GET /admin` carries `Authorization: Bearer
# <token>` (see docs/headless/docker-compose.yml).
#
# Optional: LINXIV_P2P_RELAY_URL (+ LINXIV_P2P_RELAY_TOKEN) seeds the relay
# setting on first boot so `/api/admin/node-address` can mint a Node Address
# without a manual settings PATCH. Mount a volume at /data or the node's
# identity (and that address) resets on every container restart.
FROM rust:1-bookworm AS build
WORKDIR /src
COPY scripts/ scripts/
# Runtime dlopen'd PDF library, copied into the final stage below. Ahead of the
# source COPY so editing source doesn't re-download it.
RUN bash scripts/fetch_pdfium.sh
COPY src-tauri/ src-tauri/
# Cache mounts keep the registry and target/ across rebuilds, which is what
# makes self-building hosts (an arm64 box with no published image) rebuild in
# minutes instead of re-compiling ~900 crates. The mount hides target/ from
# later stages, so the bins are copied out inside the same RUN.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/src-tauri/target,sharing=locked \
    cargo build --release --locked --manifest-path src-tauri/Cargo.toml \
      -p linxiv-server -p linxiv-cli --bin linxiv-headless --bin linxiv-cli \
    && install -Dt /out src-tauri/target/release/linxiv-headless \
                        src-tauri/target/release/linxiv-cli

FROM debian:bookworm-slim
# curl: compose healthcheck probes /api/papers.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/linxiv-headless /out/linxiv-cli /usr/local/bin/
COPY --from=build /src/src-tauri/vendor/pdfium/lib/libpdfium.so /usr/local/lib/pdfium/libpdfium.so
ENV LINXIV_PDFIUM_LIB=/usr/local/lib/pdfium/libpdfium.so \
    LINXIV_DATA_DIR=/data \
    LINXIV_HTTP_ADDR=0.0.0.0:8000
VOLUME /data
EXPOSE 8000
CMD ["linxiv-headless"]
