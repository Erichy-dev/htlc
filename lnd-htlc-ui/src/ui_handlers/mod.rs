// Re-exports and module structure for UI handlers
pub mod node;
pub mod wallet;
pub mod channels;
pub mod invoices;
pub mod preimages;

// Re-export the handler initializers for easier imports
pub use node::{init_node_status_handlers, check_node_status_timer};
pub use wallet::init_wallet_handlers;
pub use channels::init_channel_handlers;
pub use invoices::{init_invoice_handlers, check_invoice_updates_timer};
pub use preimages::init_preimage_handlers; 