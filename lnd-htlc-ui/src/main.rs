mod invoice;
mod types;
mod utils;
mod node;
mod channels;

use anyhow::Result;
use serde::Deserialize;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use types::AppState;
use utils::generate_preimage;
use node::{node_status, NodeInfo};
use channels::{ActiveChannelInfo, PendingChannelInfo};

slint::include_modules!();

// Structs for deserializing lncli listinvoices output
#[derive(Deserialize, Debug, Clone)]
struct LnCliInvoice {
    memo: String,
    r_hash: String,
    value: String, // Value is often a string in lncli output
    state: String, // e.g., "OPEN", "SETTLED"
    creation_date: String, // Unix timestamp string
    // Add other fields if needed, like amt_paid_sat, is_keysend etc.
}

#[derive(Deserialize, Debug)]
struct ListInvoicesResponse {
    invoices: Vec<LnCliInvoice>,
    // last_index_offset, first_index_offset if you need pagination
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create channel for node status updates
    let (tx_node_status, mut rx_node_status) = mpsc::channel(10);
    
    // Spawn task to check node status in intervals
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            let info = node_status();
            if let Err(_) = tx_node_status.send(info).await {
                break; 
            }
        }
    });
    
    let initial_node_info = node_status();
    let window = MainWindow::new()?;
    let window_weak = Arc::new(window.as_weak());

    update_ui_with_node_info(&window_weak, &initial_node_info);
    
    let window_weak_for_updates = window_weak.clone();
    tokio::spawn(async move {
        while let Some(info) = rx_node_status.recv().await {
            update_ui_with_node_info(&window_weak_for_updates, &info);
        }
    });

    // Handle Manage Channels click
    let window_weak_clone = window_weak.clone();
    window.on_manage_channels(move || {
        println!("Manage Channels clicked - fetching channel data...");
        let ui_handle_weak = window_weak_clone.clone(); // Keep it weak for the spawn

        tokio::spawn(async move {
            let active_channels_result = channels::list_active_channels();
            let pending_channels_result = channels::list_pending_channels();

            // Now, schedule the UI update on the Slint event loop
            let _ = slint::invoke_from_event_loop(move || {
                // This closure now executes on the Slint event loop
                // Re-upgrade inside the closure, as it's a new context, though window was upgraded just before
                if let Some(window_on_event_loop) = ui_handle_weak.upgrade() {
                    match active_channels_result {
                        Ok(active_list) => {
                            let slint_active_channels: Vec<Channel> = active_list.into_iter().map(|ac| Channel {
                                channel_id: ac.channel_id.into(),
                                remote_pubkey: ac.remote_pubkey.into(),
                                capacity: ac.capacity.into(),
                                local_balance: ac.local_balance.into(),
                                remote_balance: ac.remote_balance.into(),
                                active: ac.active,
                            }).collect();
                            window_on_event_loop.set_channels(ModelRc::new(VecModel::from(slint_active_channels)));
                            window_on_event_loop.set_status_message("Active channels loaded.".into());
                        }
                        Err(e) => {
                            println!("Error listing active channels: {}", e);
                            window_on_event_loop.set_status_message(format!("Error loading active channels: {}", e).into());
                        }
                    }

                    match pending_channels_result {
                        Ok(pending_list) => {
                            let slint_pending_channels: Vec<PendingChannel> = pending_list.into_iter().map(|pc| PendingChannel {
                                remote_pubkey: pc.remote_node_pub.into(),
                                channel_point: pc.channel_point.into(),
                                capacity: pc.capacity.into(),
                                local_balance: pc.local_balance.into(),
                                remote_balance: pc.remote_balance.into(),
                                status: pc.status.into(),
                            }).collect();
                            window_on_event_loop.set_pending_channels(ModelRc::new(VecModel::from(slint_pending_channels)));
                            let current_status = window_on_event_loop.get_status_message();
                            window_on_event_loop.set_status_message(format!("{} Pending channels loaded.", current_status).into());
                        }
                        Err(e) => {
                            println!("Error listing pending channels: {}", e);
                            let current_status = window_on_event_loop.get_status_message();
                            window_on_event_loop.set_status_message(format!("{} Error loading pending channels: {}", current_status, e).into());
                        }
                    }
                    window_on_event_loop.set_active_page(0i32); // Navigate to channels view (page 0)
                } else {
                        println!("Window disappeared before UI update could be scheduled on event loop.");
                }
            });
        });
    });

    let open_channel_weak_ref = window_weak.clone(); // Clone Arc<Weak> for the callback
    window.on_open_lightning_channel(move || { // This callback runs on the Slint thread
        println!("Auto Open Channel button clicked");
        
        // We don't upgrade `ui` here to pass into tokio::spawn.
        // Instead, clone the weak reference again for the tokio task.
        let task_weak_ref = open_channel_weak_ref.clone(); 
        
        tokio::spawn(async move { // task_weak_ref (Arc<Weak<MainWindow>>) is moved here. This is Send + Sync.
            const DEFAULT_CHANNEL_AMOUNT: u32 = 20000;
            
            // --- Stage 1: List Peers (Async/Blocking work) ---
            let list_peers_result = channels::list_peers();
            
            match list_peers_result {
                Ok(peers) => {
                    if let Some(first_peer_pubkey_str) = peers.first().map(|s| s.to_string()) { // Own the string
                        
                        // --- Stage 2: Update UI - Found Peer (on Slint thread) ---
                        let status_msg_found_peer = format!("Found peer: {}. Attempting to open channel for {} sats...", first_peer_pubkey_str, DEFAULT_CHANNEL_AMOUNT);
                        let weak_for_status_update = task_weak_ref.clone();
                        slint::invoke_from_event_loop(move || {
                            if let Some(ui) = weak_for_status_update.upgrade() {
                                ui.set_create_channel_status_message(status_msg_found_peer.into());
                            }
                        }).ok(); // .ok() to ignore error if UI is already closed

                        // --- Stage 3: Open Channel (Async/Blocking work) ---
                        let open_channel_result = channels::open_channel(&first_peer_pubkey_str, DEFAULT_CHANNEL_AMOUNT);
                        let weak_for_final_update = task_weak_ref.clone(); // Clone weak ref for the final update closure

                        match open_channel_result {
                            Ok(result) => {
                                let funding_txid = utils::extract_funding_txid_from_string(&result).unwrap_or_else(|| "N/A".to_string());
                                let success_message = format!(
                                    "Channel open success! Funding TXID: {}. You can now visit 'Manage Channels'.",
                                    funding_txid
                                );
                                // --- Stage 4a: Update UI - Success (on Slint thread) ---
                                slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = weak_for_final_update.upgrade() {
                                        ui.set_create_channel_funding_txid(funding_txid.into());
                                        ui.set_create_channel_status_message(success_message.into());
                                        ui.set_create_channel_in_progress(false);
                                    }
                                }).ok();
                            }
                            Err(e) => {
                                let error_message = format!("Failed to open channel with {}: {}", first_peer_pubkey_str, e);
                                println!("{}", error_message);
                                // --- Stage 4b: Update UI - Error Open Channel (on Slint thread) ---
                                slint::invoke_from_event_loop(move || {
                                     if let Some(ui) = weak_for_final_update.upgrade() {
                                        ui.set_create_channel_status_message(error_message.into());
                                        ui.set_create_channel_funding_txid("".into());
                                        ui.set_create_channel_in_progress(false);
                                    }
                                }).ok();
                            }
                        }
                    } else { // No peers found
                        let msg = "No peers found to auto-open a channel with.".to_string();
                        println!("{}", msg);
                        let weak_for_no_peers_update = task_weak_ref.clone();
                        // --- Update UI - No Peers (on Slint thread) ---
                        slint::invoke_from_event_loop(move || {
                            if let Some(ui) = weak_for_no_peers_update.upgrade() {
                                ui.set_create_channel_status_message(msg.into());
                                ui.set_create_channel_in_progress(false);
                            }
                        }).ok();
                    }
                }
                Err(e) => { // Failed to list peers
                    let error_message = format!("Failed to list peers: {}", e);
                    println!("{}", error_message);
                    let weak_for_list_error_update = task_weak_ref.clone();
                    // --- Update UI - List Peers Error (on Slint thread) ---
                    slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak_for_list_error_update.upgrade() {
                            ui.set_create_channel_status_message(error_message.into());
                            ui.set_create_channel_in_progress(false);
                        }
                    }).ok();
                }
            }
        });
    });

    let window_weak_clone = window_weak.clone();
    window.on_connect_peer(move |pubkey, host, port| {
        println!("Connecting to peer: {} @ {}:{}", pubkey, host, port);
        let port_num = match port.parse::<u16>() {
            Ok(p) => p,
            Err(_) => {
                if let Some(window) = window_weak_clone.upgrade() {
                    window.set_status_message(SharedString::from(
                        "Invalid port number. Please enter a valid number."
                    ));
                }
                return;
            }
        };
        let pubkey_clone = pubkey.to_string();
        let host_clone = host.to_string();
        let window_weak_for_connect = window_weak_clone.clone();
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(
                format!("Connecting to {}...", pubkey)
            ));
        }
        tokio::spawn(async move {
            let result = channels::connect_to_peer(&pubkey_clone, &host_clone, port_num);
            if let Some(window) = window_weak_for_connect.upgrade() {
                match result {
                    Ok(output) => {
                        window.set_status_message(SharedString::from(
                            format!("Successfully connected to peer: {}", pubkey_clone)
                        ));
                        println!("Connection successful: {}", output);
                    },
                    Err(e) => {
                        window.set_status_message(SharedString::from(
                            format!("Failed to connect: {}", e)
                        ));
                        println!("Connection error: {}", e);
                    }
                }
            }
        });
    });

    let window_weak_clone = window_weak.clone();
    window.on_manage_invoices(move || {
        println!("Listing invoices (UI callback invoked)...");
        let ui_handle_weak = window_weak_clone.clone();

        tokio::spawn(async move {
            match invoice::list_invoices() {
                Ok(invoices_json_str) => {
                    // Attempt to parse the JSON
                    match serde_json::from_str::<ListInvoicesResponse>(&invoices_json_str) {
                        Ok(parsed_response) => {
                            let slint_invoices_vec: Vec<InvoiceDetails> = parsed_response.invoices.into_iter().map(|i| InvoiceDetails {
                                memo: i.memo.into(),
                                r_hash: i.r_hash.into(),
                                value: i.value.into(),
                                state: i.state.into(),
                                creation_date: i.creation_date.into(), // Consider formatting this from timestamp if needed
                            }).collect();

                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(window) = ui_handle_weak.upgrade() {
                                    window.set_all_invoices(ModelRc::new(VecModel::from(slint_invoices_vec)));
                                    window.set_status_message("Invoices loaded.".into());
                                    println!("Invoices successfully loaded and UI updated.");
                                    window.set_active_page(2i32);
                                }
                            });
                        }
                        Err(e) => {
                            let error_msg = format!("Error parsing invoices JSON: {}", e);
                            println!("{}", error_msg);
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(window) = ui_handle_weak.upgrade() {
                                    window.set_status_message(error_msg.into());
                                }
                            });
                        }
                    }
                }
                Err(e) => {
                    let error_msg = format!("Error listing invoices from lncli: {}", e);
                    println!("{}", error_msg);
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = ui_handle_weak.upgrade() {
                            window.set_status_message(error_msg.into());
                        }
                    });
                }
            }
        });
    });

    let window_weak_clone = window_weak.clone();
    window.on_request_preimage_generation(move || {
        let (preimage, hash) = generate_preimage();
        if let Some(window) = window_weak_clone.upgrade() {
            window.set_status_message(SharedString::from(format!(
                "Generated preimage: {}, hash: {}",
                preimage, hash
            )));
            window.set_generated_preimage_x(SharedString::from(preimage.clone()));
            window.set_generated_preimage_h(SharedString::from(hash.clone()));
        }
    });

    let window_weak_clone = window_weak.clone();
    window.on_create_custom_invoice(move |preimage, amount, memo| {
        if let Some(window) = window_weak_clone.upgrade() {
            println!("Creating custom invoice with preimage: {}, amount: {}, memo: {}", preimage, amount, memo);
            match invoice::create_invoice(preimage.to_string(), amount.to_string(), memo.to_string()) {
                Ok(output) => {
                    window.set_status_message(SharedString::from(format!(
                        "Created invoice with preimage: {}, amount: {}, memo: {}",
                        preimage, amount, memo
                    )));
                    // Parse JSON output to get payment_addr
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output) {
                        if let Some(payment_addr) = json.get("payment_addr").and_then(|v| v.as_str()) {
                            window.set_payment_address(SharedString::from(payment_addr));
                        }
                    }
                    window.set_generated_preimage_h(SharedString::from(""));
                    window.set_generated_preimage_x(SharedString::from(""));
                }
                Err(e) => {
                    window.set_status_message(SharedString::from(format!(
                        "Error creating invoice: {}",
                        e
                    )));
                }
            }
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

fn update_ui_with_node_info(window_weak: &Arc<slint::Weak<MainWindow>>, node_info: &NodeInfo) {
    if let Some(window) = window_weak.upgrade() {
        window.set_node_is_running(node_info.running);
        if node_info.running {
            window.set_wallet_needs_unlock(false);
        }
        let sync_status = if node_info.synced {
            format!("Synced: {} \n(h: {})", node_info.network, node_info.block_height)
        } else {
            format!("Syncing: {} \n(h: {})", node_info.network, node_info.block_height)
        };
        window.set_node_sync_status(SharedString::from(sync_status));
        window.set_status_checking(true); 
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
