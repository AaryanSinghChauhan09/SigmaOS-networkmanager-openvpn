// SPDX-License-Identifier: MIT
// SigmaOS-networkmanager-openvpn - OpenVPN NetworkManager Plugin component
// OpenVPN plugin for SigmaOS NetworkManager,
// inspired by Arch's networkmanager-openvpn package.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// ============================================================================
// VPN CONFIGURATION
// ============================================================================

/// VPN connection type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpnType {
    /// OpenVPN (TCP)
    OpenVpnTcp,
    /// OpenVPN (UDP)
    OpenVpnUdp,
    /// OpenVPN over TCP with TLS
    OpenVpnTls,
    /// L2TP/IPsec (for future)
    L2tpIpsec,
    /// WireGuard (for future)
    WireGuard,
}

impl VpnType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenVpnTcp => "openvpn-tcp",
            Self::OpenVpnUdp => "openvpn-udp",
            Self::OpenVpnTls => "openvpn-tls",
            Self::L2tpIpsec => "l2tp-ipsec",
            Self::WireGuard => "wireguard",
        }
    }
}

/// VPN encryption/cipher
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cipher {
    /// AES-256-GCM
    Aes256Gcm,
    /// AES-128-GCM
    Aes128Gcm,
    /// AES-256-CBC
    Aes256Cbc,
    /// AES-128-CBC
    Aes128Cbc,
    /// None (insecure)
    None,
}

impl Cipher {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Aes256Gcm => "AES-256-GCM",
            Self::Aes128Gcm => "AES-128-GCM",
            Self::Aes256Cbc => "AES-256-CBC",
            Self::Aes128Cbc => "AES-128-CBC",
            Self::None => "none",
        }
    }
}

/// VPN authentication type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthType {
    /// Username/password
    Password,
    /// Certificate
    Certificate,
    /// TLS key + certificate
    TlsAuth,
    /// Pre-shared key
    Psk,
}

impl AuthType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Certificate => "certificate",
            Self::TlsAuth => "tls-auth",
            Self::Psk => "psk",
        }
    }
}

/// VPN connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpnState {
    /// Disconnected
    Disconnected,
    /// Connecting
    Connecting,
    /// Connected
    Connected,
    /// Disconnecting
    Disconnecting,
    /// Failed
    Failed,
}

impl VpnState {
    pub fn is_connected(&self) -> bool { *self == Self::Connected }
    pub fn is_active(&self) -> bool { matches!(self, Self::Connecting | Self::Connected | Self::Disconnecting) }
}

/// OpenVPN connection configuration
#[derive(Debug, Clone)]
pub struct OpenVpnConfig {
    /// VPN type (TCP/UDP)
    pub vpn_type: VpnType,
    /// Remote server address
    pub remote: String,
    /// Remote port
    pub port: u16,
    /// Protocol (tcp/udp)
    pub proto: String,
    /// Cipher
    pub cipher: Cipher,
    /// Authentication type
    pub auth_type: AuthType,
    /// Username (for password auth)
    pub username: Option<String>,
    /// Password (for password auth)
    pub password: Option<String>,
    /// Certificate path
    pub cert_path: Option<String>,
    /// Key path
    pub key_path: Option<String>,
    /// CA certificate path
    pub ca_path: Option<String>,
    /// TLS auth key path
    pub tls_auth_path: Option<String>,
    /// Whether to redirect all traffic
    pub redirect_all: bool,
    /// Whether to use compression
    pub comp_lzo: bool,
    /// Whether to persist key
    pub persist_key: bool,
    /// Whether to persist tun
    pub persist_tun: bool,
    /// Custom config file
    pub config_file: Option<String>,
}

impl OpenVpnConfig {
    pub fn new(remote: impl Into<String>, port: u16) -> Self {
        Self {
            vpn_type: VpnType::OpenVpnUdp,
            remote: remote.into(),
            port,
            proto: "udp".into(),
            cipher: Cipher::Aes256Gcm,
            auth_type: AuthType::Password,
            username: None,
            password: None,
            cert_path: None,
            key_path: None,
            ca_path: None,
            tls_auth_path: None,
            redirect_all: true,
            comp_lzo: false,
            persist_key: true,
            persist_tun: true,
            config_file: None,
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), VpnError> {
        if self.remote.is_empty() {
            return Err(VpnError::InvalidConfig("remote address required".into()));
        }
        if self.port == 0 || self.port > 65535 {
            return Err(VpnError::InvalidConfig("invalid port".into()));
        }
        if self.auth_type == AuthType::Password {
            if self.username.is_none() || self.password.is_none() {
                return Err(VpnError::InvalidConfig("username and password required for password auth".into()));
            }
        }
        if self.auth_type == AuthType::Certificate {
            if self.cert_path.is_none() || self.key_path.is_none() || self.ca_path.is_none() {
                return Err(VpnError::InvalidConfig("certificate, key, and CA required for certificate auth".into()));
            }
        }
        Ok(())
    }

    /// Generate OpenVPN config file content
    pub fn to_config(&self) -> String {
        let mut cfg = String::new();
        cfg.push_str(&format!("client\n"));
        cfg.push_str(&format!("dev tun\n"));
        cfg.push_str(&format!("proto {}\n", self.proto));
        cfg.push_str(&format!("remote {} {}\n", self.remote, self.port));
        cfg.push_str(&format!("cipher {}\n", self.cipher.as_str().to_lowercase()));

        if self.redirect_all {
            cfg.push_str("redirect-gateway def1\n");
        }
        if self.comp_lzo {
            cfg.push_str("comp-lzo\n");
        }
        if self.persist_key {
            cfg.push_str("persist-key\n");
        }
        if self.persist_tun {
            cfg.push_str("persist-tun\n");
        }

        match self.auth_type {
            AuthType::Password => {
                cfg.push_str(&format!("auth-user-pass\n"));
                if let Some(u) = &self.username {
                    cfg.push_str(&format!("# username: {}\n", u));
                }
            }
            AuthType::Certificate => {
                if let Some(c) = &self.cert_path {
                    cfg.push_str(&format!("cert {}\n", c));
                }
                if let Some(k) = &self.key_path {
                    cfg.push_str(&format!("key {}\n", k));
                }
                if let Some(ca) = &self.ca_path {
                    cfg.push_str(&format!("ca {}\n", ca));
                }
            }
            AuthType::TlsAuth => {
                if let Some(ta) = &self.tls_auth_path {
                    cfg.push_str(&format!("tls-auth {} 1\n", ta));
                }
            }
            AuthType::Psk => {
                cfg.push_str("auth none\n");
            }
        }

        cfg.push_str("verb 3\n"); // log level
        cfg
    }
}

// ============================================================================
// VPN CONNECTION
// ============================================================================

/// A VPN connection instance
#[derive(Debug, Clone)]
pub struct VpnConnection {
    /// Connection ID
    pub id: String,
    /// Connection name
    pub name: String,
    /// Configuration
    pub config: OpenVpnConfig,
    /// Current state
    pub state: VpnState,
    /// Connected since (ms)
    pub connected_since_ms: Option<u64>,
    /// Bytes received
    pub rx_bytes: u64,
    /// Bytes transmitted
    pub tx_bytes: u64,
    /// Remote IP (after connection)
    pub remote_ip: Option<String>,
    /// Virtual IP (tun0 address)
    pub virtual_ip: Option<String>,
    /// DNS servers pushed by VPN
    pub dns_servers: Vec<String>,
    /// Routes pushed by VPN
    pub routes: Vec<String>,
}

impl VpnConnection {
    pub fn new(name: impl Into<String>, config: OpenVpnConfig) -> Self {
        let name = name.into();
        Self {
            id: format!("vpn-{}-{}", name, now_ms()),
            name,
            config,
            state: VpnState::Disconnected,
            connected_since_ms: None,
            rx_bytes: 0,
            tx_bytes: 0,
            remote_ip: None,
            virtual_ip: None,
            dns_servers: Vec::new(),
            routes: Vec::new(),
        }
    }

    /// Connect the VPN
    pub fn connect(&mut self) -> Result<(), VpnError> {
        self.config.validate()?;
        self.state = VpnState::Connecting;
        // Simulate connection
        self.state = VpnState::Connected;
        self.connected_since_ms = Some(now_ms());
        self.virtual_ip = Some("10.8.0.2".into());
        self.remote_ip = Some(self.config.remote.clone());
        self.dns_servers = vec!["10.8.0.1".into()];
        Ok(())
    }

    /// Disconnect the VPN
    pub fn disconnect(&mut self) {
        self.state = VpnState::Disconnecting;
        self.state = VpnState::Disconnected;
        self.connected_since_ms = None;
        self.virtual_ip = None;
        self.remote_ip = None;
    }

    /// Whether connected
    pub fn is_connected(&self) -> bool {
        self.state.is_connected()
    }

    /// Uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.connected_since_ms
            .map(|t| (now_ms() - t) / 1000)
            .unwrap_or(0)
    }

    /// Total traffic in bytes
    pub fn total_bytes(&self) -> u64 {
        self.rx_bytes + self.tx_bytes
    }

    /// Update traffic counters
    pub fn add_traffic(&mut self, rx: u64, tx: u64) {
        self.rx_bytes += rx;
        self.tx_bytes += tx;
    }
}

// ============================================================================
// VPN MANAGER
// ============================================================================

/// The NetworkManager OpenVPN plugin
pub struct OpenVpnManager {
    /// Active connections
    connections: Mutex<HashMap<String, VpnConnection>>,
    /// Connection configs (saved profiles)
    profiles: Mutex<HashMap<String, OpenVpnConfig>>,
}

impl OpenVpnManager {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            profiles: Mutex::new(HashMap::new()),
        }
    }

    /// Save a VPN profile
    pub fn save_profile(&self, name: impl Into<String>, config: OpenVpnConfig) -> Result<(), VpnError> {
        config.validate()?;
        self.profiles.lock().unwrap().insert(name.into(), config);
        Ok(())
    }

    /// Load a VPN profile
    pub fn load_profile(&self, name: &str) -> Option<OpenVpnConfig> {
        self.profiles.lock().unwrap().get(name).cloned()
    }

    /// Delete a VPN profile
    pub fn delete_profile(&self, name: &str) -> bool {
        self.profiles.lock().unwrap().remove(name).is_some()
    }

    /// List saved profiles
    pub fn list_profiles(&self) -> Vec<String> {
        self.profiles.lock().unwrap().keys().cloned().collect()
    }

    /// Connect using a saved profile
    pub fn connect(&self, name: &str) -> Result<VpnConnection, VpnError> {
        let config = self.load_profile(name)
            .ok_or_else(|| VpnError::ProfileNotFound(name.to_string()))?;
        let mut conn = VpnConnection::new(name, config);
        conn.connect()?;
        let conn_clone = conn.clone();
        self.connections.lock().unwrap().insert(conn.id.clone(), conn);
        Ok(conn_clone)
    }

    /// Connect with explicit config
    pub fn connect_with_config(&self, name: &str, config: OpenVpnConfig) -> Result<VpnConnection, VpnError> {
        let mut conn = VpnConnection::new(name, config);
        conn.connect()?;
        let conn_clone = conn.clone();
        self.connections.lock().unwrap().insert(conn.id.clone(), conn);
        Ok(conn_clone)
    }

    /// Disconnect a connection
    pub fn disconnect(&self, conn_id: &str) -> Result<(), VpnError> {
        let mut conns = self.connections.lock().unwrap();
        let conn = conns.get_mut(conn_id)
            .ok_or_else(|| VpnError::ConnectionNotFound(conn_id.to_string()))?;
        conn.disconnect();
        Ok(())
    }

    /// Get connection status
    pub fn get_connection(&self, conn_id: &str) -> Option<VpnConnection> {
        self.connections.lock().unwrap().get(conn_id).cloned()
    }

    /// List all connections
    pub fn list_connections(&self) -> Vec<VpnConnection> {
        self.connections.lock().unwrap().values().cloned().collect()
    }

    /// Get active connections
    pub fn active_connections(&self) -> Vec<VpnConnection> {
        self.list_connections().into_iter()
            .filter(|c| c.is_connected())
            .collect()
    }

    /// Check if any VPN is connected
    pub fn any_connected(&self) -> bool {
        self.active_connections().len() > 0
    }
}

impl Default for OpenVpnManager {
    fn default() -> Self { Self::new() }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[derive(Debug)]
pub enum VpnError {
    InvalidConfig(String),
    ProfileNotFound(String),
    ConnectionNotFound(String),
    ConnectFailed(String),
    AlreadyConnected,
}

impl fmt::Display for VpnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(m) => write!(f, "invalid config: {}", m),
            Self::ProfileNotFound(n) => write!(f, "profile not found: {}", n),
            Self::ConnectionNotFound(n) => write!(f, "connection not found: {}", n),
            Self::ConnectFailed(m) => write!(f, "connect failed: {}", m),
            Self::AlreadyConnected => write!(f, "already connected"),
        }
    }
}
impl std::error::Error for VpnError {}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> OpenVpnConfig {
        let mut cfg = OpenVpnConfig::new("vpn.example.com", 1194);
        cfg.username = Some("user".into());
        cfg.password = Some("pass".into());
        cfg
    }

    #[test]
    fn test_config_validate() {
        let cfg = test_config();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn test_config_validate_missing() {
        let cfg = OpenVpnConfig::new("", 1194);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_config_to_file() {
        let cfg = test_config();
        let content = cfg.to_config();
        assert!(content.contains("client"));
        assert!(content.contains("remote vpn.example.com 1194"));
        assert!(content.contains("proto udp"));
        assert!(content.contains("cipher aes-256-gcm"));
        assert!(content.contains("auth-user-pass"));
        assert!(content.contains("redirect-gateway def1"));
    }

    #[test]
    fn test_config_cert_auth() {
        let mut cfg = OpenVpnConfig::new("vpn.example.com", 443);
        cfg.auth_type = AuthType::Certificate;
        cfg.cert_path = Some("/etc/openvpn/client.crt".into());
        cfg.key_path = Some("/etc/openvpn/client.key".into());
        cfg.ca_path = Some("/etc/openvpn/ca.crt".into());
        cfg.vpn_type = VpnType::OpenVpnTcp;
        cfg.proto = "tcp".into();

        assert!(cfg.validate().is_ok());
        let content = cfg.to_config();
        assert!(content.contains("proto tcp"));
        assert!(content.contains("cert /etc/openvpn/client.crt"));
        assert!(content.contains("key /etc/openvpn/client.key"));
        assert!(content.contains("ca /etc/openvpn/ca.crt"));
    }

    #[test]
    fn test_connection_lifecycle() {
        let mut conn = VpnConnection::new("test-vpn", test_config());
        assert!(!conn.is_connected());
        conn.connect().unwrap();
        assert!(conn.is_connected());
        assert!(conn.virtual_ip.is_some());
        conn.disconnect();
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_manager_profiles() {
        let mgr = OpenVpnManager::new();
        mgr.save_profile("work", test_config()).unwrap();
        assert!(mgr.load_profile("work").is_some());
        assert_eq!(mgr.list_profiles().len(), 1);
        assert!(mgr.delete_profile("work"));
        assert!(mgr.load_profile("work").is_none());
    }

    #[test]
    fn test_manager_connect() {
        let mgr = OpenVpnManager::new();
        mgr.save_profile("home", test_config()).unwrap();
        let conn = mgr.connect("home").unwrap();
        assert!(conn.is_connected());
        assert!(mgr.any_connected());
        mgr.disconnect(&conn.id).unwrap();
    }

    #[test]
    fn test_traffic() {
        let mut conn = VpnConnection::new("test", test_config());
        conn.add_traffic(1000, 500);
        conn.add_traffic(2000, 1000);
        assert_eq!(conn.rx_bytes, 3000);
        assert_eq!(conn.tx_bytes, 1500);
        assert_eq!(conn.total_bytes(), 4500);
    }

    #[test]
    fn test_vpn_types() {
        assert_eq!(VpnType::OpenVpnUdp.as_str(), "openvpn-udp");
        assert_eq!(VpnType::OpenVpnTcp.as_str(), "openvpn-tcp");
        assert_eq!(VpnState::Connected.is_connected(), true);
        assert_eq!(VpnState::Disconnected.is_connected(), false);
    }
}
