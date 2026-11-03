# Development container for Agentproof.
# It holds the Rust tools. The project files stay on the host.
FROM rust:1.99-bookworm

# Tools to build the Windows version of the engine from Linux.
RUN apt-get update \
    && apt-get install -y --no-install-recommends mingw-w64 \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustfmt clippy \
    && rustup target add x86_64-pc-windows-gnu

ENV CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    CARGO_TARGET_DIR=/target

# The project folder comes from the host, so Git must trust it.
RUN git config --system --add safe.directory /workspace

WORKDIR /workspace
