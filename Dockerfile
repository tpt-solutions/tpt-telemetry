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
RUN cargo build --release --features tpt-otlp/tls --bin tpt_daemon

FROM gcr.io/distroless/cc-debian12
COPY --from=builder \
    /usr/src/tpt-telemetry/target/release/tpt_daemon \
    /usr/local/bin/tpt-daemon
COPY --from=builder \
    /usr/src/tpt-telemetry/packaging/tpt-daemon.example.toml \
    /etc/tpt-daemon/tpt-daemon.toml
USER nonroot:nonroot
EXPOSE 514/udp
EXPOSE 514/tcp
EXPOSE 9464/tcp
# Liveness probe: the daemon serves an unauthenticated /healthz heartbeat on
# its metrics port (9464 by default). The distroless runtime has no shell, so
# the probe is performed by the daemon binary itself via `--healthcheck`.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/tpt-daemon", "--healthcheck", "--config", "/etc/tpt-daemon/tpt-daemon.toml"]
ENTRYPOINT ["/usr/local/bin/tpt-daemon"]
CMD ["--config", "/etc/tpt-daemon/tpt-daemon.toml"]
