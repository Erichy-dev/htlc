use slint::SharedString;

#[derive(Debug, Clone)]
pub struct AppState {
    // pub invoices: Vec<Invoice>,
    pub status_message: String,
    pub node_is_running: bool,
    pub node_sync_status: String,
    pub wallet_needs_unlock: bool,
    pub litd_pid: Option<u32>, // Store PID of litd process we started
}

#[derive(Debug, Clone)]
pub struct LndConnection {
    pub host: String,
    pub port: u16,
    pub cert_path: String,
    pub macaroon_path: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            // invoices: Vec::new(),
            status_message: String::new(),
            node_is_running: false,
            node_sync_status: "Unknown".to_string(),
            wallet_needs_unlock: false,
            litd_pid: None,
        }
    }
}

impl Default for LndConnection {
    fn default() -> Self {
        // For Windows, the default paths use AppData/Local/Lit
        let home_dir = dirs::home_dir().unwrap_or_default();
        let lit_dir = home_dir.join("AppData").join("Local").join("Lit");
        let cert_path = lit_dir.join("tls.cert").to_string_lossy().to_string();
        let macaroon_path = lit_dir.join("testnet").join("lit.macaroon").to_string_lossy().to_string();
        
        Self {
            host: "127.0.0.1".to_string(),
            port: 10009,
            cert_path,
            macaroon_path,
        }
    }
}

// Re-export the Invoice structure from slint for convenience
// pub use crate::Invoice; 