FROM rust:1.85-alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig

WORKDIR /build

# Copy workspace root files
COPY Cargo.toml Cargo.lock ./

# Copy the crates we actually build
COPY coast-core coast-core
COPY coast-docker coast-docker
COPY coast-remote coast-remote

# Stub out workspace members we don't need.
# coast-daemon has a build.rs, so copy it + skip UI build.
COPY coast-daemon/Cargo.toml coast-daemon/Cargo.toml
COPY coast-daemon/build.rs coast-daemon/build.rs
RUN mkdir -p coast-daemon/src && echo "fn main(){}" > coast-daemon/src/main.rs && echo "" > coast-daemon/src/lib.rs
# Create a stub bin that the daemon Cargo.toml references
RUN mkdir -p coast-daemon/src/bin && echo "fn main(){}" > coast-daemon/src/bin/coastd-dev.rs

COPY coast-cli/Cargo.toml coast-cli/Cargo.toml
RUN mkdir -p coast-cli/src && echo "fn main(){}" > coast-cli/src/main.rs && echo "" > coast-cli/src/lib.rs
RUN mkdir -p coast-cli/src/bin && echo "fn main(){}" > coast-cli/src/bin/coast-dev.rs

COPY coast-secrets/Cargo.toml coast-secrets/Cargo.toml
RUN mkdir -p coast-secrets/src && echo "" > coast-secrets/src/lib.rs

COPY coast-update/Cargo.toml coast-update/Cargo.toml
RUN mkdir -p coast-update/src && echo "" > coast-update/src/lib.rs

COPY coast-i18n/Cargo.toml coast-i18n/Cargo.toml
RUN mkdir -p coast-i18n/src && echo "" > coast-i18n/src/lib.rs

COPY integration-tests/Cargo.toml integration-tests/Cargo.toml
RUN mkdir -p integration-tests/src && echo "" > integration-tests/src/lib.rs

# Skip coast-guard UI build
ENV COAST_SKIP_UI_BUILD=1

# Pre-build dependencies (cached layer)
RUN cargo build -p coast-remote --release 2>&1 || true

# Copy all real source and rebuild
COPY . .
RUN cargo build -p coast-remote --release

# --- Runtime image ---
FROM docker:dind

RUN apk add --no-cache sshfs bash curl openssh-client

COPY --from=builder /build/target/release/coast-remote /usr/local/bin/coast-remote

RUN mkdir -p /mnt/coast

EXPOSE 31416

COPY coast-remote/test-harness/entrypoint-remote.sh /entrypoint-remote.sh
RUN chmod +x /entrypoint-remote.sh

ENTRYPOINT ["/entrypoint-remote.sh"]
