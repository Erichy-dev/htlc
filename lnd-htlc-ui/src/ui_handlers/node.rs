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
            // If wallet is locked, that means node is definitely running
            let node_is_running = is_running || wallet_locked;
            println!("DEBUG: Node status check result - is_running: {}, wallet_locked: {}, final state: {}", 
                    is_running, wallet_locked, node_is_running);
            
            {
                if let Ok(mut state) = app_state.lock() {
                    // Update the state with the correct running status
                    state.node_is_running = node_is_running;
                    state.node_sync_status = sync_status.clone();
                    state.wallet_needs_unlock = wallet_locked;
                    
                    // Update status message if wallet became locked
                    if wallet_locked {
                        state.status_message = "Lightning wallet is locked. Please unlock it with your wallet password.".to_string();
                    }
                    
                    println!("DEBUG: After state update - node_is_running: {}, wallet_locked: {}", 
                             state.node_is_running, state.wallet_needs_unlock);
                }
            }
            
            if let Some(window) = window_weak.upgrade() {
                // Temporarily print the current UI values
                let current_node_running = window.get_node_is_running();
                let current_wallet_locked = window.get_wallet_needs_unlock();
                println!("DEBUG: Current UI values BEFORE update - node_is_running: {}, wallet_locked: {}", 
                        current_node_running, current_wallet_locked);
                
                // Always update UI with the correct running status
                window.set_node_is_running(node_is_running);
                window.set_node_sync_status(SharedString::from(sync_status));
                window.set_wallet_needs_unlock(wallet_locked);
                
                // After updating, read back values to confirm they changed
                let new_node_running = window.get_node_is_running();
                let new_wallet_locked = window.get_wallet_needs_unlock();
                println!("DEBUG: UI values AFTER update - node_is_running: {}, wallet_locked: {}", 
                        new_node_running, new_wallet_locked);
                
                // Make sure the start node button is hidden if the node is running
                if node_is_running {
                    window.set_litd_started_by_app(true);
                    println!("DEBUG: Explicitly setting UI node_is_running to true because node is running");
                }
                
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
                    let node_is_running = is_running || wallet_locked;  // If wallet is locked, node is running
                    println!("DEBUG: INITIAL NODE STATUS - raw is_running: {}, wallet_locked: {}, effective is_running: {}", 
                            is_running, wallet_locked, node_is_running);
                    
                    {
                        if let Ok(mut state) = app_state.lock() {
                            state.node_is_running = node_is_running;  // Use the combined flag
                            state.node_sync_status = sync_status.clone();
                            state.wallet_needs_unlock = wallet_locked;
                            if !node_is_running {
                                state.status_message = "Lightning node (lnd) is not running. Please start litd using: litd --network testnet".to_string();
                            }
                        }
                    }
                    
                    if let Some(window) = window_weak.upgrade() {
                        window.set_node_is_running(node_is_running);  // Use the combined flag
                        window.set_node_sync_status(SharedString::from(sync_status));
                        window.set_wallet_needs_unlock(wallet_locked);
                        
                        // Always forcibly hide the start button if wallet is locked (node must be running)
                        if wallet_locked {
                            window.set_litd_started_by_app(true);
                            println!("DEBUG: Forcibly hiding start button due to wallet locked state");
                        }
                        
                        if !node_is_running {
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
                        let node_is_running = is_running || wallet_locked;  // If wallet is locked, node is running
                        println!("DEBUG: CHECK STATUS - raw is_running: {}, wallet_locked: {}, effective is_running: {}", 
                                is_running, wallet_locked, node_is_running);
                        
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.node_is_running = node_is_running;
                                state.node_sync_status = sync_status.clone();
                                state.wallet_needs_unlock = wallet_locked;
                                state.status_message = "Node status checked".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_node_is_running(node_is_running);
                            window.set_node_sync_status(SharedString::from(sync_status));
                            window.set_wallet_needs_unlock(wallet_locked);
                            
                            // Always forcibly hide the start button if wallet is locked (node must be running)
                            if wallet_locked {
                                window.set_litd_started_by_app(true);
                                println!("DEBUG: Forcibly hiding start button due to wallet locked state (check button)");
                            }
                            
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
                    Ok(pid) => {
                        let status_msg = format!("Started Lightning node (PID: {})", pid);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = status_msg.clone();
                                state.litd_pid = Some(pid);
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(status_msg));
                            window.set_litd_started_by_app(true);
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