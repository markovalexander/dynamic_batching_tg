FROM lukemathwalker/cargo-chef:latest-rust-1.78 AS chef
WORKDIR /usr/src

ARG CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

FROM chef as planner
COPY Cargo.toml Cargo.toml
COPY rust-toolchain.toml rust-toolchain.toml
COPY proto proto
COPY router router
COPY launcher launcher
COPY server server
COPY telegram_bot telegram_bot
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

RUN PROTOC_ZIP=protoc-21.12-linux-x86_64.zip && \
    curl -OL https://github.com/protocolbuffers/protobuf/releases/download/v21.12/$PROTOC_ZIP && \
    unzip -o $PROTOC_ZIP -d /usr/local bin/protoc && \
    unzip -o $PROTOC_ZIP -d /usr/local 'include/*' && \
    rm -f $PROTOC_ZIP

COPY --from=planner /usr/src/recipe.json recipe.json
RUN cargo chef cook --profile release-opt --recipe-path recipe.json

ARG GIT_SHA
ARG DOCKER_LABEL

COPY Cargo.toml Cargo.toml
COPY rust-toolchain.toml rust-toolchain.toml
COPY proto proto
COPY router router
COPY launcher launcher
COPY server server
COPY telegram_bot telegram_bot
RUN cargo build --profile release-opt

FROM ubuntu:20.04 AS runtime

# Install router
COPY --from=builder /usr/src/target/release-opt/router /usr/local/bin/router
# Install launcher
COPY --from=builder /usr/src/target/release-opt/launcher /usr/local/bin/launcher
# Install telegram_bot
COPY --from=builder /usr/src/target/release-opt/telegram_bot /usr/local/bin/telegram_bot
# Install server
COPY --from=builder /usr/src/target/release-opt/server /usr/local/bin/server

CMD ["launcher", "localhost", "8000", "50051"]
