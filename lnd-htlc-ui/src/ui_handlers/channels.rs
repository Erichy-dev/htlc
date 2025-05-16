use slint::{SharedString, ComponentHandle};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::MainWindow;
use crate::types::AppState;
use crate::invoice::list_channels;

// Initialize channel handlers
pub fn init_channel_handlers(window: &MainWindow, app_state: &Arc<Mutex<AppState>>) {
    // Handle manage channels button
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_manage_channels(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            thread::spawn(move || {
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.status_message = "Listing channels...".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.set_status_message(SharedString::from("Listing channels..."));
                    // active-page property is already set in the UI callback
                }
                
                match list_channels() {
                    Ok(_output) => {
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = "Channels listed".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from("Channels listed"));
                        }
                    },
                    Err(e) => {
                        let error_msg = format!("Error listing channels: {}", e);
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
        let _app_state = app_state.clone();
        let _window_weak = window.as_weak();
        window.on_create_channel(move || {
            // The actual active-page property is already set in the UI callback
            // We don't need to set it here
        });
    }
    
    // TODO: Implement actual channel creation callback handler
} 