mod invoice;
mod types;
mod utils;
mod node;
mod channels;
mod litd_service;
mod unlock_wallet;
mod mac_service;
mod windows_service;

use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use slint::{Model, ModelRc, SharedString, VecModel};
use unlock_wallet::unlock_wallet_rpc;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use chrono::{DateTime, NaiveDateTime, Utc};

use sha2::Digest;

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
    payment_request: String,
    // Add other fields if needed, like amt_paid_sat, is_keysend etc.
}

#[derive(Deserialize, Debug)]
struct ListInvoicesResponse {
    invoices: Vec<LnCliInvoice>,
    // last_index_offset, first_index_offset if you need pagination
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InvoiceData {
    preimage_x: String,
    preimage_h: String,
    payment_address: String,
    r_hash: String,
    is_own_invoice: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let app_data_dir = get_app_data_dir().unwrap();
    // Initialize sled database
    let db = match sled::open(app_data_dir.join("invoice_data_db")) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("CRITICAL: Failed to open sled database 'invoice_data_db': {}. Please check permissions and disk space.", e);
            panic!("Failed to open database: {}", e);
        }
    };

    let initial_network_db = db.clone();
    let initial_network = initial_network_db.get(b"network")?.unwrap_or(sled::IVec::from(b"testnet"));
    let initial_network_str = String::from_utf8(initial_network.to_vec()).unwrap_or_else(|_| "testnet".to_string());

    let initial_network_str_clone = initial_network_str.clone();
    // Start litd service
    match litd_service::start_litd_service(&initial_network_str_clone) {
        Ok(_) => {
            // Create channel for node status updates
            let (tx_node_status, mut rx_node_status) = mpsc::channel(5);
            let window = MainWindow::new().map_err(|e| anyhow::anyhow!("Failed to create main window: {}", e))?;
            let window_weak = Arc::new(window.as_weak());

            let node_db = db.clone();
            let node_update_window_clone = window_weak.clone();
            // Spawn task to check node status in intervals
            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    let node_network = litd_service::get_network(&node_db).await.unwrap_or_else(|_| "testnet".to_string());
                    let info = node_status(&node_network, &node_update_window_clone).await;
                    if let Err(_) = tx_node_status.send(info).await {
                        break; 
                    }
                }
            });
            
            let initial_network_str_clone = initial_network_str.clone();
            let initial_node_window_clone = window_weak.clone();
            let initial_node_info = node_status(&initial_network_str_clone, &initial_node_window_clone).await;

            let node_db_clone = db.clone();
            update_ui_with_node_info(&window_weak, initial_node_info.clone(), &node_db_clone);
            if !initial_node_info.running {
                window.set_wallet_needs_unlock(true);
            }
            
            let window_weak_for_updates = window_weak.clone();
            let node_update_db_clone = db.clone();
            tokio::spawn(async move {
                while let Some(info) = rx_node_status.recv().await {
                    update_ui_with_node_info(&window_weak_for_updates, info, &node_update_db_clone);
                }
            });

            let network_window_weak = window_weak.clone();
            let network_db = db.clone();
            window.on_toggle_network(move |network: SharedString| {
                println!("Toggling network: {}", network);
                let network_str = network.to_string();
                let task_arc_weak_clone = network_window_weak.clone();
                let network_db_clone = network_db.clone();

                tokio::spawn(async move {
                    match litd_service::stop_litd_service() {
                        Ok(_) => {
                            println!("Litd service stopped successfully");
                            tokio::time::sleep(Duration::from_secs(3)).await;
                            
                            match litd_service::start_litd_service(&network_str) {
                                Ok(_) => {
                                    println!("Litd service started successfully");
                                    
                                    network_db_clone.insert(b"network", network_str.as_bytes());
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(window) = task_arc_weak_clone.upgrade() {
                                            window.set_is_mainnet(network_str == "mainnet");
                                        }
                                    });
                                }
                                Err(e) => {
                                    println!("Failed to start litd service: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("Failed to stop litd service: {}", e);
                        }
                    }
                });
            });

            let wallet_window_weak = window_weak.clone();
            window.on_unlock_wallet(move |password: SharedString| {
                println!("Unlocking wallet with password: {}", password);
                let password_str = password.to_string();
                let task_arc_weak_clone = wallet_window_weak.clone(); // Clone Arc for the tokio task

                tokio::spawn(async move { // task_arc_weak_clone is moved into this async block
                    match unlock_wallet_rpc(&password_str).await {
                        Ok(_) => {
                            println!("Wallet unlocked successfully");
                            // Clone the Arc again for the invoke_from_event_loop closure
                            let invoke_arc_weak_clone_ok = task_arc_weak_clone.clone();
                            let _ = slint::invoke_from_event_loop(move || { // invoke_arc_weak_clone_ok moved here
                                if let Some(window_on_event_loop) = invoke_arc_weak_clone_ok.upgrade() {
                                    window_on_event_loop.set_wallet_needs_unlock(false);
                                    window_on_event_loop.set_active_page(-1i32);
                                } else {
                                    println!("Could not update UI after wallet unlock: window closed.");
                                }
                            });
                        }
                        Err(e) => {
                            println!("Failed to unlock wallet: {}", e);
                            let error_message = format!("Failed to unlock wallet: {}", e);
                            // Clone the Arc again for this invoke_from_event_loop closure
                            let invoke_arc_weak_clone_err = task_arc_weak_clone.clone();
                            let _ = slint::invoke_from_event_loop(move || { // invoke_arc_weak_clone_err and error_message moved here
                                if let Some(window_on_event_loop) = invoke_arc_weak_clone_err.upgrade() {
                                    window_on_event_loop.set_status_message(SharedString::from(error_message));
                                } else {
                                    println!("Could not update status after wallet unlock failure: window closed.");
                                }
                            });
                        }
                    }
                });
            });

            // Handle Manage Channels click
            let window_weak_clone = window_weak.clone();
            let channels_db = db.clone();
            window.on_manage_channels(move || {
                println!("Manage Channels clicked - fetching channel data...");
                let ui_handle_weak = window_weak_clone.clone(); // Keep it weak for the spawn
                let channels_db_clone = channels_db.clone();
                
                tokio::spawn(async move {
                    let channels_network = litd_service::get_network(&channels_db_clone).await.unwrap_or_else(|_| "testnet".to_string());

                    let active_channels_result = channels::list_active_channels(&channels_network);
                    let pending_channels_result = channels::list_pending_channels(&channels_network);

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
            let channel_db = db.clone();
            window.on_open_lightning_channel(move || { // This callback runs on the Slint thread
                println!("Auto Open Channel button clicked");
                
                // We don't upgrade `ui` here to pass into tokio::spawn.
                // Instead, clone the weak reference again for the tokio task.
                let task_weak_ref = open_channel_weak_ref.clone(); 
                let channel_db_clone = channel_db.clone();
                
                tokio::spawn(async move { // task_weak_ref (Arc<Weak<MainWindow>>) is moved here. This is Send + Sync.
                    const DEFAULT_CHANNEL_AMOUNT: u32 = 20000;

                    let channel_network = litd_service::get_network(&channel_db_clone).await.unwrap_or_else(|_| "testnet".to_string());
                    
                    // --- Stage 1: List Peers (Async/Blocking work) ---
                    let list_peers_result = channels::list_peers(&channel_network);
                    
                    let channel_network_clone = channel_network.clone();
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
                                let open_channel_result = channels::open_channel(&channel_network_clone, &first_peer_pubkey_str, DEFAULT_CHANNEL_AMOUNT);
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
            let connect_db = db.clone();
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
                let connect_db_clone = connect_db.clone();

                if let Some(window) = window_weak_clone.upgrade() {
                    window.set_status_message(SharedString::from(
                        format!("Connecting to {}...", pubkey)
                    ));
                }
                tokio::spawn(async move {
                    let connect_network = litd_service::get_network(&connect_db_clone).await.unwrap_or_else(|_| "testnet".to_string());
                    let result = channels::connect_to_peer(&connect_network, &pubkey_clone, &host_clone, port_num);
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
            let db_clone = db.clone();

            window.on_manage_invoices(move || {
                println!("Listing invoices (UI callback invoked)...");
                let ui_handle_weak = window_weak_clone.clone();
                let db_clone_for_invoices = db_clone.clone();

                tokio::spawn(async move {
                    match invoice::list_invoices(&db_clone_for_invoices) {
                        Ok(slint_invoices_vec) => {
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
            let db_clone_for_create = db.clone();

            window.on_create_custom_invoice(move |preimage_x, preimage_h, amount, memo| {
                if let Some(window) = window_weak_clone.upgrade() {
                    println!("Creating custom invoice with preimage: {}, amount: {}, memo: {}", preimage_x, amount, memo);
                    match invoice::create_invoice(preimage_x.to_string(), preimage_h.to_string(), amount.to_string(), memo.to_string(), &db_clone_for_create) {
                        Ok(output) => {
                            window.set_status_message(SharedString::from(format!(
                                "Created invoice with preimage: {}, amount: {}, memo: {}",
                                preimage_x, amount, memo
                            )));
                            window.set_payment_address(SharedString::from(output.payment_addr));
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


            let confirm_preimage_window_weak_clone = window_weak.clone();
            window.on_confirm_preimage(move | pre_image_x: SharedString, pre_image_h: SharedString | {
                if let Some(window) = confirm_preimage_window_weak_clone.upgrade() {
                    let x_bytes = match hex::decode(pre_image_x.to_string()) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            window.set_custom_invoice_status_message(SharedString::from(format!("Invalid preimage X")));
                            window.set_confirmed_preimage(false);
                            return;
                        }
                    };

                    let mut hasher = Sha256::new();
                    hasher.update(x_bytes);
                    let result = hasher.finalize();
                    let result_hex = hex::encode(result);
                    if result_hex == pre_image_h.to_string() {
                        window.set_custom_invoice_status_message(SharedString::from("Preimage confirmed."));
                        window.set_confirmed_preimage(true);
                    } else {
                        let status_message = format!("Preimage does not match.\n\nPreimage X: {}\nPreimage H: {}\nHash: {}", pre_image_x, pre_image_h, result_hex);
                        println!("{}", status_message);
                        window.set_custom_invoice_status_message(SharedString::from(status_message));
                        window.set_confirmed_preimage(false);
                    }
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

            let settle_window_weak_clone = window_weak.clone();
            let db_clone_for_settle = db.clone();
            window.on_settle_custom_invoice(move |preimage_h| {
                if let Some(window) = settle_window_weak_clone.upgrade() {
                    window.set_status_message(SharedString::from(format!(
                        "Settling invoice with preimage hash: {}",
                        preimage_h
                    )));

                    let refresh_ui_handle_weak = settle_window_weak_clone.clone();
                    let refresh_db_clone = db_clone_for_settle.clone();

                    match invoice::settle_invoice(preimage_h.to_string(), &db_clone_for_settle) {
                        Ok(_) => {
                            window.set_status_message(SharedString::from("Invoice settled successfully. Refreshing list..."));

                            // Spawn a task to refresh the invoices list
                            tokio::spawn(async move {
                                match invoice::list_invoices(&refresh_db_clone) {
                                    Ok(slint_invoices_vec) => {
                                        let _ = slint::invoke_from_event_loop(move || {
                                            if let Some(window_for_refresh) = refresh_ui_handle_weak.upgrade() {
                                                window_for_refresh.set_all_invoices(ModelRc::new(VecModel::from(slint_invoices_vec)));
                                                let current_status = window_for_refresh.get_status_message();
                                                window_for_refresh.set_status_message(format!("{} Invoices refreshed.", current_status).into());
                                                println!("Invoices successfully refreshed and UI updated after settlement.");
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        let error_msg = format!("Error refreshing invoices after settlement: {}", e);
                                        println!("{}", error_msg);
                                        let _ = slint::invoke_from_event_loop(move || {
                                            if let Some(window_for_refresh_err) = refresh_ui_handle_weak.upgrade() {
                                                let current_status = window_for_refresh_err.get_status_message();
                                                window_for_refresh_err.set_status_message(format!("{} Failed to refresh invoices: {}", current_status, e).into());
                                            }
                                        });
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            window.set_status_message(SharedString::from(format!(
                                "Error settling invoice: {}",
                                e
                            )));
                        }
                    }
                }
            });

            let copy_window_weak_clone = window_weak.clone();
            window.on_copy_to_clipboard(move |payment_request| {
                if let Some(window) = copy_window_weak_clone.upgrade() {
                    match invoice::copy_payment_request(payment_request.to_string()) {
                        Ok(_) => {
                            window.set_status_message(SharedString::from(format!(
                                "Copied to clipboard",
                            )));
                        }
                        Err(e) => {
                            window.set_status_message(SharedString::from(format!(
                                "Error copying payment request to clipboard: {}",
                                e
                            )));
                        }
                    }
                }
            });

            let standard_window_weak_clone = window_weak.clone();
            let db_clone_for_create = db.clone();

            window.on_create_standard_invoice(move |amount, memo| {
                if let Some(window) = standard_window_weak_clone.upgrade() {
                    println!("Creating standard invoice with amount: {}, memo: {}", amount, memo);

                    match invoice::create_standard_invoice(amount.to_string(), memo.to_string(), &db_clone_for_create) {
                        Ok(output) => {
                            window.set_status_message(SharedString::from(format!(
                                "Created standard invoice with memo: {}, amount: {}",
                                memo, amount
                            )));
                            window.set_standard_payment_address(SharedString::from(output));
                        }
                        Err(e) => {
                            window.set_status_message(SharedString::from(format!(
                                "Error creating standard invoice: {}",
                                e
                            )));
                        }
                    }
                }
            });

            window.run()?;
            Ok(())
        }
        Err(e) => {
            println!("Error starting litd service: {}", e);
            return Err(e);
        }
    }
}

fn update_ui_with_node_info(window_weak: &Arc<slint::Weak<MainWindow>>, node_info: NodeInfo, db: &sled::Db) {
    let window_weak_clone = window_weak.clone();
    let db_clone = db.clone();

    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = window_weak_clone.upgrade() {
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

            let timer_window_weak = window.as_weak(); // Use window's own weak ref for timer
            slint::Timer::single_shot(Duration::from_secs(1), move || {
                if let Some(window) = timer_window_weak.upgrade() {
                    window.set_status_checking(false);
                }
            });

            if node_info.running {
                db_clone.insert(b"identity_pubkey", node_info.identity_pubkey.as_bytes());
                window.set_status_message(SharedString::from(
                    format!("Connected to LND {}", node_info.identity_pubkey),
                ));
            } else {
                window.set_status_message(SharedString::from(
                    "Node is not running",
                ));
            }
        }
    }).map_err(|e| {
        eprintln!("Failed to invoke UI update from event loop: {:?}. This might happen if the UI is already closed.", e);
    });
}

pub fn get_app_data_dir() -> Option<PathBuf> {
    if let Some(proj_dirs) = ProjectDirs::from("com", "btc", "lnd-htlc-ui") {
        let data_dir = proj_dirs.data_dir();
        return Some(data_dir.to_path_buf());
    }
    None
}
