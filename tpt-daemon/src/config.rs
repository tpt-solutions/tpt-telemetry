//! TOML configuration for the `tpt-daemon`.
//!
//! `${ENV_VAR}` interpolation is applied to string fields (paths, binds, the OTLP
//! endpoint and header values, the metrics bind, and the log level) so secrets
//! such as `OTLP_TOKEN` can be kept out of the config file.

use serde::Deserialize;
use std::collections::HashMap;
use tpt_otlp::{ExporterConfig, Transport};
use tpt_syslog_server::{ServerConfig, TcpFraming};

/// Top-level daemon configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct DaemonConfig {
    pub schema: SchemaConfig,
    pub syslog: SyslogConfig,
    pub otlp: OtlpConfig,
    pub metrics: MetricsConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchemaConfig {
    /// Path to a `.tpt-log` schema file.
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyslogConfig {
    pub udp: BindConfig,
    pub tcp: TcpConfig,
    #[serde(default = "default_ring_capacity")]
    pub ring_capacity: usize,
    #[serde(default = "default_read_timeout")]
    pub read_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BindConfig {
    pub bind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TcpConfig {
    pub bind: String,
    #[serde(default)]
    pub framing: FramingMode,
    #[serde(default = "default_max_frame_len")]
    pub max_frame_len: usize,
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FramingMode {
    #[default]
    Auto,
    Octet,
    Lf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OtlpConfig {
    pub endpoint: String,
    #[serde(default)]
    pub transport: OtlpTransport,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_retries")]
    pub max_retries: usize,
    #[serde(default = "default_backoff")]
    pub base_backoff_ms: u64,
    #[serde(default = "default_scope")]
    pub scope_name: String,
    #[serde(default)]
    pub require_tls: bool,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OtlpTransport {
    #[default]
    Http,
    Grpc,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    pub bind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_level")]
    pub level: String,
}

impl DaemonConfig {
    /// Replace `${ENV_VAR}` references in all string fields with the matching
    /// process environment variable (left verbatim if unset).
    pub fn interpolate_env(&mut self) {
        self.schema.path = subst(&self.schema.path);
        self.syslog.udp.bind = subst(&self.syslog.udp.bind);
        self.syslog.tcp.bind = subst(&self.syslog.tcp.bind);
        self.otlp.endpoint = subst(&self.otlp.endpoint);
        for v in self.otlp.headers.values_mut() {
            *v = subst(v);
        }
        self.metrics.bind = subst(&self.metrics.bind);
        self.logging.level = subst(&self.logging.level);
    }

    /// Build the [`ServerConfig`] consumed by `tpt-syslog-server`.
    pub fn server_config(&self) -> ServerConfig {
        ServerConfig {
            udp_bind: self
                .syslog
                .udp
                .bind
                .parse()
                .expect("invalid syslog.udp.bind address"),
            tcp_bind: self
                .syslog
                .tcp
                .bind
                .parse()
                .expect("invalid syslog.tcp.bind address"),
            ring_capacity: self.syslog.ring_capacity,
            read_timeout_ms: self.syslog.read_timeout_ms,
            tcp_framing: self.syslog.tcp.framing.into(),
            max_connections: self.syslog.tcp.max_connections,
            max_frame_len: self.syslog.tcp.max_frame_len,
        }
    }

    /// Build the [`ExporterConfig`] consumed by `tpt-otlp`.
    pub fn exporter_config(&self) -> ExporterConfig {
        ExporterConfig {
            transport: self.otlp.transport.into(),
            endpoint: self.otlp.endpoint.clone(),
            headers: self.otlp.headers.clone(),
            batch_size: self.otlp.batch_size,
            timeout_ms: self.otlp.timeout_ms,
            max_retries: self.otlp.max_retries,
            base_backoff_ms: self.otlp.base_backoff_ms,
            scope_name: self.otlp.scope_name.clone(),
            require_tls: self.otlp.require_tls,
        }
    }
}

impl From<FramingMode> for TcpFraming {
    fn from(f: FramingMode) -> Self {
        match f {
            FramingMode::Auto => TcpFraming::Auto,
            FramingMode::Octet => TcpFraming::OctetCounting,
            FramingMode::Lf => TcpFraming::NonTransparent,
        }
    }
}

impl From<OtlpTransport> for Transport {
    fn from(t: OtlpTransport) -> Self {
        match t {
            OtlpTransport::Http => Transport::Http,
            OtlpTransport::Grpc => Transport::Grpc,
        }
    }
}

/// Substitute `${NAME}` occurrences with the value of environment variable
/// `NAME`, leaving the literal `${NAME}` in place when the variable is unset.
fn subst(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut name = String::new();
            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(x) => name.push(x),
                    None => {
                        out.push_str("${");
                        out.push_str(&name);
                        return out;
                    }
                }
            }
            match std::env::var(&name) {
                Ok(v) => out.push_str(&v),
                Err(_) => {
                    out.push_str("${");
                    out.push_str(&name);
                    out.push('}');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn default_ring_capacity() -> usize {
    1 << 16
}
fn default_read_timeout() -> u64 {
    250
}
fn default_max_frame_len() -> usize {
    1_048_576
}
fn default_max_connections() -> usize {
    1024
}
fn default_batch_size() -> usize {
    1024
}
fn default_timeout() -> u64 {
    10_000
}
fn default_retries() -> usize {
    3
}
fn default_backoff() -> u64 {
    100
}
fn default_scope() -> String {
    "tpt-daemon".into()
}
fn default_level() -> String {
    "info".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_env_vars() {
        std::env::set_var("TPT_TEST_TOKEN", "s3cr3t");
        let mut cfg = DaemonConfig {
            schema: SchemaConfig {
                path: "schema.tpt-log".into(),
            },
            syslog: SyslogConfig {
                udp: BindConfig {
                    bind: "127.0.0.1:${TPT_TEST_PORT}".into(),
                },
                tcp: TcpConfig {
                    bind: "127.0.0.1:0".into(),
                    framing: FramingMode::Auto,
                    max_frame_len: 1,
                    max_connections: 1,
                },
                ring_capacity: 1,
                read_timeout_ms: 1,
            },
            otlp: OtlpConfig {
                endpoint: "http://localhost".into(),
                transport: OtlpTransport::Http,
                batch_size: 1,
                timeout_ms: 1,
                max_retries: 1,
                base_backoff_ms: 1,
                scope_name: "x".into(),
                require_tls: false,
                headers: HashMap::from([(
                    "Authorization".into(),
                    "Bearer ${TPT_TEST_TOKEN}".into(),
                )]),
            },
            metrics: MetricsConfig {
                bind: "127.0.0.1:0".into(),
            },
            logging: LoggingConfig {
                level: "info".into(),
            },
        };
        cfg.interpolate_env();
        assert_eq!(
            cfg.otlp.headers.get("Authorization").unwrap(),
            "Bearer s3cr3t"
        );
        assert!(cfg.syslog.udp.bind.contains("127.0.0.1:"));
        std::env::remove_var("TPT_TEST_TOKEN");
    }

    #[test]
    fn leaves_unset_env_vars_verbatim() {
        let mut s = "http://${DOES_NOT_EXIST_XYZ}/v1/logs".to_string();
        s = subst(&s);
        assert_eq!(s, "http://${DOES_NOT_EXIST_XYZ}/v1/logs");
    }
}
