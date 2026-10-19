FROM rust:1.97-bookworm AS build
WORKDIR /source
COPY . .
RUN cargo build --release --locked -p horiplay-signaling
FROM debian:bookworm-slim
RUN useradd --system --uid 10001 horiplay
COPY --from=build /source/target/release/horiplay-signaling /usr/local/bin/horiplay-signaling
USER 10001
ENV HORIPLAY_SIGNAL_ADDR=0.0.0.0:8787
EXPOSE 8787
ENTRYPOINT ["horiplay-signaling"]
