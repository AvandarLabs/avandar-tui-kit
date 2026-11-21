# Linux floor image for Layer 1, Layer 3, Layer 4, and Linux CI jobs.
# Pins Rust 1.88, Debian Bookworm's tmux 3.3a, Node.js 22 LTS, and Playwright Chromium 1.65.1.

FROM rust:1.88-bookworm

ARG NODE_VERSION=22.15.0
ARG NODE_SHA256_ARM64=d68adf72c531f1118bee75b20ffbc5911accfda5e73454a798625464b40a4adf
ARG NODE_SHA256_AMD64=dafe2e8f82cb97de1bd10db9e2ec4c07bbf53389b0799b1e095a918951e78fd4
ARG JUST_VERSION=1.40.0
ARG JUST_SHA256_ARM64=d065d0df1a1f99529869fba8a5b3e0a25c1795b9007099b00dfabe29c7c1f7b6
ARG JUST_SHA256_AMD64=181b91d0ceebe8a57723fb648ed2ce1a44d849438ce2e658339df4f8db5f1263

RUN rustup component add rustfmt clippy

RUN DEBIAN_FRONTEND=noninteractive apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        fonts-dejavu-core \
        fonts-liberation \
        fonts-noto-cjk \
        fonts-noto-color-emoji \
        g++ \
        git \
        make \
        pkg-config \
        procps \
        python3 \
        tmux \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    dpkg_arch="$(dpkg --print-architecture)"; \
    case "$dpkg_arch" in \
        arm64) \
            node_arch="linux-arm64"; \
            node_sha256="$NODE_SHA256_ARM64"; \
            just_target="aarch64-unknown-linux-musl"; \
            just_sha256="$JUST_SHA256_ARM64"; \
            ;; \
        amd64) \
            node_arch="linux-x64"; \
            node_sha256="$NODE_SHA256_AMD64"; \
            just_target="x86_64-unknown-linux-musl"; \
            just_sha256="$JUST_SHA256_AMD64"; \
            ;; \
        *) \
            echo "Unsupported Debian architecture: $dpkg_arch" >&2; \
            exit 1; \
            ;; \
    esac; \
    curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-${node_arch}.tar.xz" -o /tmp/node.tar.xz; \
    echo "${node_sha256}  /tmp/node.tar.xz" | sha256sum -c -; \
    tar -xJf /tmp/node.tar.xz -C /usr/local --strip-components=1 --no-same-owner; \
    rm -f /tmp/node.tar.xz; \
    curl -fsSL "https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-${just_target}.tar.gz" -o /tmp/just.tar.gz; \
    echo "${just_sha256}  /tmp/just.tar.gz" | sha256sum -c -; \
    tar -xzf /tmp/just.tar.gz -C /usr/local/bin just; \
    chmod 0755 /usr/local/bin/just; \
    rm -f /tmp/just.tar.gz; \
    rustc --version | grep -E '^rustc 1\.88\.'; \
    tmux -V | grep -E '^tmux 3\.3'; \
    node --version | grep -E '^v22\.'; \
    just --version

ENV PLAYWRIGHT_BROWSERS_PATH=/ms-playwright

RUN DEBIAN_FRONTEND=noninteractive npx --yes playwright@1.65.1 install --with-deps chromium \
    && rm -rf /var/lib/apt/lists/* /root/.npm \
    && chmod -R a+rX /ms-playwright

COPY rig/xterm/package.json rig/xterm/package-lock.json /opt/tuikit-xterm-deps/

RUN cd /opt/tuikit-xterm-deps \
    && npm ci \
    && sha256sum package-lock.json | awk '{print $1}' > /opt/tuikit-xterm-deps/.linux-lock.sha256 \
    && rm -rf /root/.npm \
    && chmod -R a+rX /opt/tuikit-xterm-deps

RUN mkdir -p /var/cache/tuikit/cargo-target /var/cache/tuikit/cargo-home \
    && chmod -R 0777 /var/cache/tuikit \
    && chmod -R a+rwX /usr/local/cargo \
    && chmod -R a+rX /usr/local/rustup

COPY rig/linux/entrypoint.sh /usr/local/bin/tuikit-linux-entrypoint.sh
RUN chmod 0755 /usr/local/bin/tuikit-linux-entrypoint.sh

WORKDIR /workspace
ENTRYPOINT ["/usr/local/bin/tuikit-linux-entrypoint.sh"]
