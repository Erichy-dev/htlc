mod invoice;
mod types;
mod utils;
mod node;

use anyhow::Result;
use slint::SharedString;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use types::AppState;
use utils::generate_preimage;
use node::{node_status, NodeInfo};

slint::include_modules!();

#[tokio::main]
async fn main() -> Result<()> {
    // Create channel for node status updates
    let (tx, mut rx) = mpsc::channel(10);
    
    // Spawn task to check node status in intervals
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(10));
        
        loop {
            interval.tick().await;
            let info = node_status();
            if let Err(_) = tx.send(info).await {
                break; // Exit if receiver dropped
            }
        }
    });
    
    // Initial node status check
    let initial_node_info = node_status();

    let window = MainWindow::new()?;
    let window_weak = Arc::new(window.as_weak());

    // Set up UI with initial node info
    update_ui_with_node_info(&window_weak, &initial_node_info);
    
    // Set up task to update UI when new status arrives
    let window_weak_for_updates = window_weak.clone();
    tokio::spawn(async move {
        while let Some(info) = rx.recv().await {
            update_ui_with_node_info(&window_weak_for_updates, &info);
        }
    });

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

// Helper function to update UI with node info
fn update_ui_with_node_info(window_weak: &Arc<slint::Weak<MainWindow>>, node_info: &NodeInfo) {
    if let Some(window) = window_weak.upgrade() {
        window.set_node_is_running(node_info.running);
        
        let sync_status = if node_info.synced {
            format!("Synced: {} \n(h: {})", node_info.network, node_info.block_height)
        } else {
            format!("Syncing: {} \n(h: {})", node_info.network, node_info.block_height)
        };
        
        window.set_node_sync_status(SharedString::from(sync_status));
        window.set_status_checking(true); // Trigger animation
        
        // Reset animation after a short delay
        let window_weak_clone = window_weak.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Some(window) = window_weak_clone.upgrade() {
                window.set_status_checking(false);
            }
        });
        
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
}
