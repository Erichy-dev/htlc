mod invoice;
mod types;
mod utils;
mod node;

use anyhow::Result;
use slint::SharedString;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use types::AppState;
use utils::generate_preimage;
use node::{node_status, NodeInfo};

slint::include_modules!();

#[tokio::main]
async fn main() -> Result<()> {
    // Spawn node status check in separate thread
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        let info = node_status();
        let _ = tx.send(info); // Ignore error if receiver dropped
    });

    let window = MainWindow::new()?;
    let window_weak = Arc::new(window.as_weak());

    // Get node status from background thread
    let node_info = match rx.await {
        Ok(info) => info,
        Err(_) => {
            println!("Failed to get node status from background thread");
            // Default values if channel fails
            NodeInfo {
                running: false,
                version: "unknown".to_string(),
                synced: false,
                block_height: 0,
                network: "testnet".to_string(),
            }
        }
    };

    // Set up UI with node info
    if let Some(window) = window_weak.upgrade() {
        window.set_node_is_running(node_info.running);
        
        let sync_status = if node_info.synced {
            format!("Synced: {} (h: {})", node_info.network, node_info.block_height)
        } else {
            format!("Syncing: {} (h: {})", node_info.network, node_info.block_height)
        };
        
        window.set_node_sync_status(SharedString::from(sync_status));
        window.set_wallet_needs_unlock(false);
        window.set_litd_started_by_app(false);
        
        if node_info.running {
            window.set_status_message(SharedString::from(
                format!("Connected to LND v{}", node_info.version),
            ));
        } else {
            window.set_status_message(SharedString::from(
                "UI Demo Mode - API connectivity disabled",
            ));
        }
    }

    // Set up callbacks for UI navigation - don't try to set active-page directly
    window.on_manage_channels(move || {
        println!("Demo: Manage Channels clicked");
    });

    window.on_create_channel(move || {
        println!("Demo: Create Channel clicked");
    });

    let window_weak_clone = window_weak.clone();
    window.on_generate_xh(move || {
        // Generate a demo preimage/hash pair
        let (preimage, hash) = generate_preimage();

        if let Some(window) = window_weak_clone.upgrade() {
            window.invoke_update_preimage_hash(
                SharedString::from(preimage.clone()),
                SharedString::from(hash.clone()),
            );

            window.set_status_message(SharedString::from(format!(
                "Demo: Generated preimage: {}, hash: {}",
                preimage, hash
            )));
        }
    });

    let window_weak_clone = window_weak.clone();
    window.on_create_custom_invoice(move |preimage, amount, memo| {
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(format!(
                "Demo: Created invoice with preimage: {}, amount: {}, memo: {}",
                preimage, amount, memo
            )));
        }
    });

    let window_weak_clone = window_weak.clone();
    window.on_pay_custom_invoice(move |bolt11| {
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(format!(
                "Demo: Paid invoice: {}",
                bolt11
            )));
        }
    });

    let window_weak_clone = window_weak.clone();
    window.on_claim_custom_invoice(move |hash, preimage| {
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(format!(
                "Demo: Claimed invoice with hash: {}, preimage: {}",
                hash, preimage
            )));
        }
    });

    let window_weak_clone = window_weak.clone();
    window.on_create_standard_invoice(move |memo, amount| {
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(format!(
                "Demo: Created standard invoice with memo: {}, amount: {}",
                memo, amount
            )));
        }
    });

    window.run()?;
    Ok(())
}
