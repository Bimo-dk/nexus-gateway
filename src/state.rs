use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HostFramework {
    Auto,
    Angular,
    Vue,
    React,
}

impl std::fmt::Display for HostFramework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostFramework::Auto => write!(f, "auto"),
            HostFramework::Angular => write!(f, "angular"),
            HostFramework::Vue => write!(f, "vue"),
            HostFramework::React => write!(f, "react"),
        }
    }
}

impl Default for HostFramework {
    fn default() -> Self {
        HostFramework::Auto
    }
}

// ---- Protection config (mirrored from registry GatewayProtectionConfig) ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectionConfig {
    pub rate_limit_enabled: bool,
    pub rate_limit_requests_per_second: u32,
    pub rate_limit_burst: u32,
    pub rate_limit_by: String,
    pub max_connections_per_ip: u32,
    pub max_websocket_connections_per_ip: u32,
    pub request_timeout_ms: u64,
    pub header_read_timeout_ms: u64,
    pub body_read_timeout_ms: u64,
    pub idle_timeout_ms: u64,
    pub max_body_bytes: u64,
    pub max_header_bytes: u64,
    pub slowloris_timeout_ms: u64,
    pub ban_duration_seconds: u64,
    pub ban_threshold_violations: u32,
}

impl Default for ProtectionConfig {
    fn default() -> Self {
        Self {
            rate_limit_enabled: true,
            rate_limit_requests_per_second: 100,
            rate_limit_burst: 200,
            rate_limit_by: "ip".into(),
            max_connections_per_ip: 50,
            max_websocket_connections_per_ip: 5,
            request_timeout_ms: 30_000,
            header_read_timeout_ms: 5_000,
            body_read_timeout_ms: 10_000,
            idle_timeout_ms: 60_000,
            max_body_bytes: 1_048_576,
            max_header_bytes: 8_192,
            slowloris_timeout_ms: 10_000,
            ban_duration_seconds: 300,
            ban_threshold_violations: 10,
        }
    }
}

// ---- Gateway-wide config ----

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GatewayConfig {
    pub cors_origins: Vec<String>,
    pub custom_headers: Vec<CustomHeader>,
    pub health_check_path: Option<String>,
    pub public_url: Option<String>,
    #[serde(default)]
    pub protection: ProtectionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct GatewayState {
    pub gate_id: String,
    pub gate_name: String,
    pub host_id: String,
    pub host_name: String,
    pub host_url: String,
    pub host_framework: HostFramework,
    pub host_remote_entry: String,
    pub host_exposed_module: String,
    pub gateway_config: GatewayConfig,
    pub registry_url: String,
    pub nexus_token: String,
    pub registry_connected: bool,
}

impl GatewayState {
    pub fn health_check_path(&self) -> &str {
        self.gateway_config
            .health_check_path
            .as_deref()
            .unwrap_or("/health")
    }
}

pub type SharedState = Arc<RwLock<GatewayState>>;

pub fn new_shared(state: GatewayState) -> SharedState {
    Arc::new(RwLock::new(state))
}

// ---- Registry API response shapes ----

#[derive(Debug, Deserialize)]
pub struct RegistryGate {
    pub id: String,
    pub name: String,
    pub host: Option<RegistryHost>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHost {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub framework: HostFramework,
    #[serde(default)]
    pub remote_entry: String,
    #[serde(default)]
    pub exposed_module: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRemote {
    pub name: String,
    pub url: String,
    pub route_path: String,
    pub visibility: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct RegistryGatewayConfig {
    #[serde(default)]
    pub cors_origins: Vec<String>,
    #[serde(default)]
    pub custom_headers: Vec<CustomHeader>,
    pub health_check_path: Option<String>,
    pub public_url: Option<String>,
    #[serde(default)]
    pub protection: Option<ProtectionConfig>,
}
