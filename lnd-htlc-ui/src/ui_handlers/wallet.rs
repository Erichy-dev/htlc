use slint::{SharedString, ComponentHandle};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::MainWindow;
use crate::types::AppState;
use crate::wallet::unlock_wallet;

// Initialize wallet handlers
pub fn init_wallet_handlers(window: &MainWindow, app_state: &Arc<Mutex<AppState>>) {
    // Handle wallet unlock
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_unlock_wallet(move |password| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            let password = password.to_string();
            
            thread::spawn(move || {
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.status_message = "Unlocking wallet...".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.set_status_message(SharedString::from("Unlocking wallet..."));
                }
                
                match unlock_wallet(&password) {
                    Ok(result) => {
                        let status_msg = format!("Wallet unlock result: {}", result);
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = status_msg.clone();
                                state.wallet_needs_unlock = false;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(status_msg));
                            window.set_wallet_needs_unlock(false);
                            window.invoke_close_wallet_dialog();
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
} 