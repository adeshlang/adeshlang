# ==============================================================================
# Multi-stage Dockerfile for AdeshLang
# Official High-Performance Systems & Application Programming Language
# ==============================================================================

# ------------------------------------------------------------------------------
# Stage 1: Build AdeshLang binaries and standard library
# ------------------------------------------------------------------------------
FROM rust:1-bookworm AS builder

# Install C/C++ compilation tools and dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libffi-dev \
    clang \
    make \
    gcc \
    g++ \
    ca-certificates \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/adeshlang

# Copy source trees and workspace configuration
COPY Cargo.toml Cargo.lock build.rs ./
COPY src/ ./src/
COPY als/ ./als/
COPY editor/ ./editor/
COPY crates/ ./crates/
COPY mobile/ ./mobile/
COPY benches/ ./benches/
COPY lib/ ./lib/
COPY installer/ ./installer/
COPY LICENSE ./
COPY THIRD_PARTY_LICENSES/ ./THIRD_PARTY_LICENSES/

# Build all release binaries and shared/static libraries
RUN cargo build --release --bin adesh --bin adl --bin als --bin adesh-editor --lib

# Stage the complete installation into /opt/adeshlang
RUN mkdir -p /opt/adeshlang/bin \
             /opt/adeshlang/std \
             /opt/adeshlang/lib \
             /opt/adeshlang/include \
             /opt/adeshlang/config \
             /opt/adeshlang/licenses \
             /opt/adeshlang/THIRD_PARTY_LICENSES && \
    cp target/release/adesh /opt/adeshlang/bin/ && \
    cp target/release/adl /opt/adeshlang/bin/ && \
    cp target/release/als /opt/adeshlang/bin/ && \
    cp target/release/adesh-editor /opt/adeshlang/bin/ && \
    ( [ -f target/release/libadeshlang.so ] && cp target/release/libadeshlang.so /opt/adeshlang/lib/ || true ) && \
    ( [ -f target/release/libadeshlang.a ] && cp target/release/libadeshlang.a /opt/adeshlang/lib/ || true ) && \
    cp -r src/stdlib/* /opt/adeshlang/std/ && \
    ( [ -f installer/manifests/toolchain-manifest.json ] && cp installer/manifests/toolchain-manifest.json /opt/adeshlang/config/ || true ) && \
    cp LICENSE /opt/adeshlang/licenses/ && \
    cp LICENSE /opt/adeshlang/THIRD_PARTY_LICENSES/

# ------------------------------------------------------------------------------
# Stage 2: Production Runtime Environment
# ------------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

LABEL maintainer="Adesh Team <support@adeshlang.org>"
LABEL org.opencontainers.image.title="AdeshLang"
LABEL org.opencontainers.image.description="Modern, high-performance, memory-safe systems programming language"
LABEL org.opencontainers.image.url="https://adeshlang.org"
LABEL org.opencontainers.image.source="https://github.com/adeshlang/adeshlang"
LABEL org.opencontainers.image.vendor="AdeshLang Community"
LABEL org.opencontainers.image.licenses="AdeshLang-2.0"

# Install runtime utilities, dynamic linker dependencies, C/C++ compiler, and complete LLVM/Clang/LLD/MLIR toolchain
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    libffi8 \
    gcc \
    g++ \
    libc6-dev \
    make \
    clang \
    lld \
    llvm \
    llvm-dev \
    procps \
    && rm -rf /var/lib/apt/lists/*

# Copy AdeshLang installation from builder stage
COPY --from=builder /opt/adeshlang /opt/adeshlang

# Create symlinks in /usr/local/bin for global accessibility
RUN ln -sf /opt/adeshlang/bin/adesh /usr/local/bin/adesh && \
    ln -sf /opt/adeshlang/bin/adl /usr/local/bin/adl && \
    ln -sf /opt/adeshlang/bin/als /usr/local/bin/als && \
    ln -sf /opt/adeshlang/bin/adesh-editor /usr/local/bin/adesh-editor

# Copy entrypoint script
COPY scripts/docker/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Setup non-root user and workspace
RUN groupadd -g 1000 adesh && \
    useradd -u 1000 -g adesh -m -s /bin/bash adesh && \
    mkdir -p /workspace /home/adesh/.adl /home/adesh/.adesh_cache && \
    chown -R adesh:adesh /workspace /home/adesh

# Environment configuration
ENV ADESH_HOME=/opt/adeshlang
ENV PATH="/opt/adeshlang/bin:${PATH}"
ENV LANG=C.UTF-8
ENV LC_ALL=C.UTF-8

WORKDIR /workspace

# Switch to non-root user by default
USER adesh

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["repl"]
