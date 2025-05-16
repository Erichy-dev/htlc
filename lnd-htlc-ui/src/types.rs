use slint::SharedString;

#[derive(Debug, Clone)]
pub struct AppState {
    pub invoices: Vec<Invoice>,
    pub status_message: String,
    pub node_is_running: bool,
    pub node_sync_status: String,
    pub wallet_needs_unlock: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            invoices: Vec::new(),
            status_message: String::new(),
            node_is_running: false,
            node_sync_status: "Unknown".to_string(),
            wallet_needs_unlock: false,
        }
    }
}

// Re-export the Invoice structure from slint for convenience
pub use crate::Invoice; 