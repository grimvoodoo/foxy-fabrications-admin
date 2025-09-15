# ─── STAGE 1: Build your Rust admin app ─────────────────────────────────────
FROM rust:1 AS builder

# Install musl tools
RUN apt-get update && apt-get install -y musl-tools && rm -rf /var/lib/apt/lists/*

# Install musl target
RUN rustup target add x86_64-unknown-linux-musl

# Create app directory
WORKDIR /app

# Copy over files
COPY . /app

# Build admin app with rustls (no OpenSSL dependency)
RUN cargo build --release --target x86_64-unknown-linux-musl

# ─── STAGE 2: Minimal runtime ───────────────────────────────────────────────

FROM scratch
# Copy statically-linked binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/foxy-fabrications-admin /

# Copy over web resources like css, templates etc.
COPY --from=builder /app/static /static
COPY --from=builder /app/templates /templates

EXPOSE 3000
ENTRYPOINT ["./foxy-fabrications-admin"]