mod invoice;
mod types;
mod utils;
mod node;
mod channels;

use anyhow::Result;
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
                    println!("Slint invoke: Active Ch: {}, Pending Ch: {}. Navigating to page 0.",
                    window_on_event_loop.get_channels().iter().len(),
                    window_on_event_loop.get_pending_channels().iter().len());
                    window_on_event_loop.set_active_page(0i32); // Navigate to channels view (page 0)
                } else {
                        println!("Window disappeared before UI update could be scheduled on event loop.");
                }
            });
        });
    });

    let window_weak_clone = window_weak.clone();
    window.on_create_channel(move || {
        println!("Create Channel View clicked");
        let window_weak_for_channel = window_weak_clone.clone();
        tokio::spawn(async move {
            if let Some(window) = window_weak_for_channel.upgrade() {
                window.set_status_message(SharedString::from(
                    "Automatically opening channel with a peer..."
                ));
            }
            match channels::auto_open_channel(20000) {
                Ok(result) => {
                    if let Some(window) = window_weak_for_channel.upgrade() {
                        window.set_status_message(SharedString::from(
                            "Channel opened successfully!"
                        ));
                    }
                    println!("Auto channel result: {}", result);
                },
                Err(e) => {
                    if let Some(window) = window_weak_for_channel.upgrade() {
                        window.set_status_message(SharedString::from(
                            format!("Failed to open channel: {}", e)
                        ));
                    }
                    println!("Auto channel error: {}", e);
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
    window.on_generate_xh(move || {
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
