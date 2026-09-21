FROM rust:1.98.1-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
RUN cargo build --locked --release -p paylink-emulator
FROM debian:bookworm-slim
RUN useradd --uid 10001 --create-home emulator
COPY --from=build /src/target/release/paylink-emulator /usr/local/bin/paylink-emulator
USER emulator
WORKDIR /home/emulator
ENTRYPOINT ["paylink-emulator"]
CMD ["serve"]
