use anyhow::Result;
use slint::{ModelRc, SharedString, VecModel, Weak, ComponentHandle};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use chrono;

use crate::MainWindow;
use crate::types::{AppState, Invoice};
use crate::node::{check_node_status, start_lightning_node};
use crate::wallet::unlock_wallet;
use crate::invoice::{
    check_invoice, create_invoice, create_standard_invoice, list_channels,
    open_channel, pay_invoice, settle_invoice,
};
use crate::utils::generate_preimage;

// Handler for checking node status
pub fn check_node_status_timer(app_state: &Arc<Mutex<AppState>>, window_weak: &Weak<MainWindow>) {
    match check_node_status() {
        Ok((is_running, sync_status, wallet_locked)) => {
            {
                if let Ok(mut state) = app_state.lock() {
                    // Only update if status changed
                    if state.node_is_running != is_running || 
                        state.node_sync_status != sync_status ||
                        state.wallet_needs_unlock != wallet_locked {
                        
                        state.node_is_running = is_running;
                        state.node_sync_status = sync_status.clone();
                        state.wallet_needs_unlock = wallet_locked;
                        
                        // Update status message if wallet became locked
                        if wallet_locked {
                            state.status_message = "Lightning wallet is locked. Please unlock it with your wallet password.".to_string();
                        }
                    }
                }
            }
            
            if let Some(window) = window_weak.upgrade() {
                window.set_node_is_running(is_running);
                window.set_node_sync_status(SharedString::from(sync_status));
                window.set_wallet_needs_unlock(wallet_locked);
                
                // Update status message if wallet is locked
                if wallet_locked {
                    window.set_status_message(SharedString::from(
                        "Lightning wallet is locked. Please unlock it with your wallet password."
                    ));
                }
            }
        },
        Err(_) => {
            {
                if let Ok(mut state) = app_state.lock() {
                    if state.node_is_running {
                        state.node_is_running = false;
                        state.node_sync_status = "Offline".to_string();
                        state.wallet_needs_unlock = false;
                    }
                }
            }
            
            if let Some(window) = window_weak.upgrade() {
                window.set_node_is_running(false);
                window.set_node_sync_status(SharedString::from("Offline"));
                window.set_wallet_needs_unlock(false);
            }
        }
    }
}

// Handler for checking invoice updates
pub fn check_invoice_updates_timer(app_state: &Arc<Mutex<AppState>>, window_weak: &Weak<MainWindow>) {
    let app_state = app_state.clone();
    let window_weak = window_weak.clone();
    
    // First, collect the pending invoices and their hashes
    let pending_hashes: Vec<String> = {
        if let Ok(state) = app_state.lock() {
            state.invoices
                .iter()
                .filter(|i| i.state == "PENDING")
                .map(|i| i.hash.to_string())
                .collect()
        } else {
            return;
        }
    };

    // Check each pending invoice
    let mut updates = Vec::new();
    for hash in pending_hashes {
        if let Ok(output) = check_invoice(&hash) {
            if output.contains("\"state\": \"ACCEPTED\"") {
                updates.push((hash.clone(), "ACCEPTED"));
            } else if output.contains("\"state\": \"SETTLED\"") {
                updates.push((hash.clone(), "SETTLED"));
            }
        }
    }

    // Apply updates in a single lock
    if !updates.is_empty() {
        let invoices_clone;
        {
            if let Ok(mut state) = app_state.lock() {
                for (hash, new_state) in updates {
                    if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
                        invoice.state = SharedString::from(new_state);
                    }
                }
                
                state.status_message = "Updated invoice states".to_string();
                invoices_clone = state.invoices.clone();
            } else {
                return;
            }
        }
        
        if let Some(window) = window_weak.upgrade() {
            window.set_invoices(ModelRc::new(VecModel::from(
                invoices_clone,
            )));
            window.set_status_message(SharedString::from("Updated invoice states"));
        }
    }
}

// Initialize node status handlers
pub fn init_node_status_handlers(window: &MainWindow, app_state: &Arc<Mutex<AppState>>) {
    // Check node status at startup
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        
        thread::spawn(move || {
            match check_node_status() {
                Ok((is_running, sync_status, wallet_locked)) => {
                    {
                        if let Ok(mut state) = app_state.lock() {
                            state.node_is_running = is_running;
                            state.node_sync_status = sync_status.clone();
                            state.wallet_needs_unlock = wallet_locked;
                            if !is_running {
                                state.status_message = "Lightning node (lnd) is not running. Please start litd using: litd --network testnet".to_string();
                            }
                        }
                    }
                    
                    if let Some(window) = window_weak.upgrade() {
                        window.set_node_is_running(is_running);
                        window.set_node_sync_status(SharedString::from(sync_status));
                        window.set_wallet_needs_unlock(wallet_locked);
                        if !is_running {
                            window.set_status_message(SharedString::from(
                                "Lightning node (lnd) is not running. Please start litd using: litd --network testnet"
                            ));
                        } else if wallet_locked {
                            window.set_status_message(SharedString::from(
                                "Lightning wallet is locked. Please unlock it with your wallet password (this is the password from your lit.conf file)."
                            ));
                        }
                    }
                },
                Err(e) => {
                    let error_msg = format!("Error checking node status: {}. Make sure litd is running.", e);
                    {
                        if let Ok(mut state) = app_state.lock() {
                            state.status_message = error_msg.clone();
                            state.node_is_running = false;
                            state.node_sync_status = "Error".to_string();
                        }
                    }
                    
                    if let Some(window) = window_weak.upgrade() {
                        window.set_status_message(SharedString::from(error_msg));
                        window.set_node_is_running(false);
                        window.set_node_sync_status(SharedString::from("Error"));
                    }
                }
            }
        });
    }

    // Handle check node status button
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_check_node_status(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                match check_node_status() {
                    Ok((is_running, sync_status, wallet_locked)) => {
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.node_is_running = is_running;
                                state.node_sync_status = sync_status.clone();
                                state.wallet_needs_unlock = wallet_locked;
                                state.status_message = "Node status updated".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_node_is_running(is_running);
                            window.set_node_sync_status(SharedString::from(sync_status));
                            window.set_wallet_needs_unlock(wallet_locked);
                            window.set_status_message(SharedString::from("Node status updated"));
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Error checking node status: {}. Make sure litd is running.", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                                state.node_is_running = false;
                                state.node_sync_status = "Error".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                            window.set_node_is_running(false);
                            window.set_node_sync_status(SharedString::from("Error"));
                        }
                    }
                }
            });
        });
    }

    // Handle start node button
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_start_node(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                match start_lightning_node() {
                    Ok(()) => {
                        let status_msg = "Starting Lightning node in a new terminal window. Please wait a few moments for it to initialize.";
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = status_msg.to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(status_msg));
                        }
                        
                        // Wait a bit and then check the status
                        thread::sleep(Duration::from_secs(5));
                        
                        match check_node_status() {
                            Ok((is_running, sync_status, wallet_locked)) => {
                                {
                                    if let Ok(mut state) = app_state.lock() {
                                        state.node_is_running = is_running;
                                        state.node_sync_status = sync_status.clone();
                                        state.wallet_needs_unlock = wallet_locked;
                                        if is_running {
                                            state.status_message = "Lightning node started successfully".to_string();
                                        } else {
                                            state.status_message = "Lightning node still starting. Check status again in a moment.".to_string();
                                        }
                                    }
                                }
                                
                                if let Some(window) = window_weak.upgrade() {
                                    window.set_node_is_running(is_running);
                                    window.set_node_sync_status(SharedString::from(sync_status));
                                    window.set_wallet_needs_unlock(wallet_locked);
                                    if is_running {
                                        window.set_status_message(SharedString::from("Lightning node started successfully"));
                                    } else {
                                        window.set_status_message(SharedString::from("Lightning node still starting. Check status again in a moment."));
                                    }
                                }
                            },
                            Err(e) => {
                                let error_msg = format!("Error checking node after start: {}", e);
                                {
                                    if let Ok(mut state) = app_state.lock() {
                                        state.status_message = error_msg.clone();
                                    }
                                }
                                
                                if let Some(window) = window_weak.upgrade() {
                                    window.set_status_message(SharedString::from(error_msg));
                                }
                            }
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Error starting Lightning node: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }
}

// Initialize wallet handlers
pub fn init_wallet_handlers(window: &MainWindow, app_state: &Arc<Mutex<AppState>>) {
    // Handle unlock wallet button
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_unlock_wallet(move |password| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                let status_msg = "Attempting to unlock wallet...";
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.status_message = status_msg.to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.set_status_message(SharedString::from(status_msg));
                }
                
                match unlock_wallet(&password) {
                    Ok(success) => {
                        let (status_msg, wallet_needs_unlock) = if success {
                            ("Wallet unlocked successfully".to_string(), false)
                        } else {
                            ("Failed to unlock wallet. Check your password and try again.".to_string(), true)
                        };
                        
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = status_msg.clone();
                                state.wallet_needs_unlock = wallet_needs_unlock;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(status_msg));
                            window.set_wallet_needs_unlock(wallet_needs_unlock);
                        }
                        
                        // If unlock was successful, update status after a short delay
                        if success {
                            thread::sleep(Duration::from_secs(1));
                            
                            // Perform a complete check of node status
                            match check_node_status() {
                                Ok((is_running, sync_status, wallet_locked)) => {
                                    {
                                        if let Ok(mut state) = app_state.lock() {
                                            state.node_is_running = is_running;
                                            state.node_sync_status = sync_status.clone();
                                            // Update wallet_needs_unlock with the actual status
                                            state.wallet_needs_unlock = wallet_locked;
                                            
                                            if wallet_locked {
                                                // Still locked after unlock attempt
                                                state.status_message = "Wallet still appears to be locked. Try again with the correct password.".to_string();
                                            }
                                        }
                                    }
                                    
                                    if let Some(window) = window_weak.upgrade() {
                                        window.set_node_is_running(is_running);
                                        window.set_node_sync_status(SharedString::from(sync_status));
                                        window.set_wallet_needs_unlock(wallet_locked);
                                        
                                        if wallet_locked {
                                            // Still locked after unlock attempt
                                            window.set_status_message(SharedString::from(
                                                "Wallet still appears to be locked. Try again with the correct password."
                                            ));
                                        }
                                    }
                                },
                                Err(e) => {
                                    let error_msg = format!("Error checking node status: {}", e);
                                    if let Some(window) = window_weak.upgrade() {
                                        window.set_status_message(SharedString::from(error_msg));
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Error unlocking wallet: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }

    // Handle close wallet dialog
    {
        let window_weak = window.as_weak();
        window.on_close_wallet_dialog(move || {
            let window_weak = window_weak.clone();
            
            if let Some(window) = window_weak.upgrade() {
                // Hide the dialog by invoking the close callback
                window.invoke_close_wallet_dialog();
            }
        });
    }
}

// Initialize channel handlers
pub fn init_channel_handlers(window: &MainWindow, app_state: &Arc<Mutex<AppState>>) {
    // Handle manage channels
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_manage_channels(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                match list_channels() {
                    Ok(channels_json) => {
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = "Channels retrieved successfully".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from("Channels retrieved successfully"));
                            // Here we would parse and display the channels data
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }

    // Handle create channel
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_create_channel(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            // This just updates the UI to show the create channel page
            {
                if let Ok(mut state) = app_state.lock() {
                    state.status_message = "Enter node public key and amount to open a channel".to_string();
                }
            }
            
            if let Some(window) = window_weak.upgrade() {
                window.set_status_message(SharedString::from("Enter node public key and amount to open a channel"));
            }
        });
    }
}

// Initialize preimage handlers
pub fn init_preimage_handlers(window: &MainWindow, app_state: &Arc<Mutex<AppState>>) {
    // Handle generate x/h
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_generate_xh(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                let (preimage, hash) = generate_preimage();
                
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.preimage_output = preimage.clone();
                        state.hash_output = hash.clone();
                        state.status_message = "Generated new preimage and hash".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    // Update the text content via a callback that we'll implement in the UI
                    window.invoke_update_preimage_hash(
                        SharedString::from(preimage), 
                        SharedString::from(hash)
                    );
                    window.set_status_message(SharedString::from("Generated new preimage and hash"));
                }
            });
        });
    }
}

// Initialize invoice handlers
pub fn init_invoice_handlers(window: &MainWindow, app_state: &Arc<Mutex<AppState>>) {
    // Handle create custom invoice
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_create_custom_invoice(move |preimage, amount, memo| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                match create_invoice(preimage.to_string(), amount.to_string(), memo.to_string()) {
                    Ok((bolt11, hash, amount)) => {
                        let new_invoice = Invoice {
                            hash: SharedString::from(hash.clone()),
                            amount: SharedString::from(amount.to_string()),
                            memo: SharedString::from(memo.to_string()),
                            preimage: SharedString::from(preimage.to_string()),
                            state: SharedString::from("PENDING"),
                            payment_request: SharedString::from(bolt11),
                            created_at: SharedString::from(format!("{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))),
                        };
                        let status_message = format!("Created invoice for {} sats", amount);
                        
                        let invoices_clone;
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.invoices.push(new_invoice);
                                state.status_message = status_message.clone();
                                invoices_clone = state.invoices.clone();
                            } else {
                                return;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_invoices(ModelRc::new(VecModel::from(
                                invoices_clone,
                            )));
                            window.set_status_message(SharedString::from(status_message));
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }
    
    // Handle pay custom invoice
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_pay_custom_invoice(move |bolt11| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                match pay_invoice(&bolt11) {
                    Ok(()) => {
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = "Payment sent successfully!".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from("Payment sent successfully!"));
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }

    // Handle claim custom invoice
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_claim_custom_invoice(move |hash, preimage| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                match settle_invoice(&hash, &preimage) {
                    Ok(()) => {
                        let invoices_clone;
                        {
                            if let Ok(mut state) = app_state.lock() {
                                if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
                                    invoice.state = SharedString::from("SETTLED");
                                }
                                state.status_message = "Invoice settled successfully!".to_string();
                                invoices_clone = state.invoices.clone();
                            } else {
                                return;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_invoices(ModelRc::new(VecModel::from(
                                invoices_clone,
                            )));
                            window.set_status_message(SharedString::from("Invoice settled successfully!"));
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }
    
    // Handle create standard invoice
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_create_standard_invoice(move |memo, amount| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                match create_standard_invoice(memo.to_string(), amount) {
                    Ok((bolt11, hash)) => {
                        let new_invoice = Invoice {
                            hash: SharedString::from(hash.clone()),
                            amount: SharedString::from(amount.to_string()),
                            memo: SharedString::from(memo.to_string()),
                            preimage: SharedString::from(""),  // Standard invoices don't expose preimage
                            state: SharedString::from("PENDING"),
                            payment_request: SharedString::from(bolt11),
                            created_at: SharedString::from(format!("{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))),
                        };
                        let status_message = format!("Created standard invoice for {} sats", amount);
                        let invoices_clone;
                        
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.invoices.push(new_invoice);
                                state.status_message = status_message.clone();
                                invoices_clone = state.invoices.clone();
                            } else {
                                return;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_invoices(ModelRc::new(VecModel::from(
                                invoices_clone,
                            )));
                            window.set_status_message(SharedString::from(status_message));
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = error_msg.clone();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(error_msg));
                        }
                    }
                }
            });
        });
    }
} 