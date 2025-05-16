use slint::{SharedString, Weak, ComponentHandle};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::MainWindow;
use crate::types::AppState;
use crate::node::{check_node_status, start_lightning_node};

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
                                state.status_message = "Node status checked".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_node_is_running(is_running);
                            window.set_node_sync_status(SharedString::from(sync_status));
                            window.set_wallet_needs_unlock(wallet_locked);
                            window.set_status_message(SharedString::from("Node status checked"));
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
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.status_message = "Starting Lightning node...".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.set_status_message(SharedString::from("Starting Lightning node..."));
                }
                
                match start_lightning_node() {
                    Ok(_) => {
                        let status_msg = "Started Lightning node";
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = status_msg.to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(status_msg));
                        }
                        
                        // Wait a moment for litd to initialize and then check status
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        check_node_status_timer(&app_state, &window_weak);
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