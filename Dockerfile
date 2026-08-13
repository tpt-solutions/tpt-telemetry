# syntax=docker/dockerfile:1
#
# Multi-stage build for the `tpt-daemon` unified syslog -> OTLP exporter.
# The builder compiles with the `tls` feature (enables OTLP/gRPC over https);
# the runtime image is a minimal distroless root, run as the non-root `nonroot`
# user. The daemon binds 514/udp + 514/tcp (syslog) and 9464/tcp (metrics);
# grant CAP_NET_BIND_SERVICE when those privileged ports are used.

FROM rust:stable-slim AS builder
WORKDIR /usr/src/tpt-telemetry
COPY . .
RUN cargo build --release --features tpt-otlp/tls --bin tpt-daemon

FROM gcr.io/distroless/cc-debian12
COPY --from=builder \
    /usr/src/tpt-telemetry/target/release/tpt-daemon \
    /usr/local/bin/tpt-daemon
COPY --from=builder \
    /usr/src/tpt-telemetry/packaging/tpt-daemon.example.toml \
    /etc/tpt-daemon/tpt-daemon.toml
USER nonroot:nonroot
EXPOSE 514/udp
EXPOSE 514/tcp
EXPOSE 9464/tcp
ENTRYPOINT ["/usr/local/bin/tpt-daemon"]
CMD ["--config", "/etc/tpt-daemon/tpt-daemon.toml"]
