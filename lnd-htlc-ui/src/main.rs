mod types;
mod utils;
mod invoice;

use anyhow::Result;
use slint::SharedString;
use std::sync::{Arc, Mutex};

use types::AppState;
use utils::generate_preimage;

slint::include_modules!();

#[tokio::main]
async fn main() -> Result<()> {
    let app_state = Arc::new(Mutex::new(AppState::default()));
    let window = MainWindow::new()?;
    let window_weak = window.as_weak();

    // Set up UI in demo mode
    if let Some(window) = window_weak.upgrade() {
        window.set_node_is_running(true);
        window.set_node_sync_status(SharedString::from("Demo Mode"));
        window.set_wallet_needs_unlock(false);
        window.set_litd_started_by_app(true);
        window.set_status_message(SharedString::from(
            "UI Demo Mode - API connectivity disabled"
        ));
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
                SharedString::from(hash.clone())
            );
            
            window.set_status_message(SharedString::from(
                format!("Demo: Generated preimage: {}, hash: {}", 
                        preimage, hash)
            ));
        }
    });

    let window_weak_clone = window_weak.clone();
    window.on_create_custom_invoice(move |preimage, amount, memo| {
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(
                format!("Demo: Created invoice with preimage: {}, amount: {}, memo: {}", 
                        preimage, amount, memo)
            ));
        }
    });

    let window_weak_clone = window_weak.clone();
    window.on_pay_custom_invoice(move |bolt11| {
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(
                format!("Demo: Paid invoice: {}", bolt11)
            ));
        }
    });

    let window_weak_clone = window_weak.clone();
    window.on_claim_custom_invoice(move |hash, preimage| {
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(
                format!("Demo: Claimed invoice with hash: {}, preimage: {}", hash, preimage)
            ));
        }
    });

    let window_weak_clone = window_weak.clone();
    window.on_create_standard_invoice(move |memo, amount| {
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(
                format!("Demo: Created standard invoice with memo: {}, amount: {}", memo, amount)
            ));
        }
    });

    window.run()?;
    Ok(())
}
