use slint::{SharedString, ComponentHandle};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::MainWindow;
use crate::types::AppState;
use crate::utils::generate_preimage;

// Initialize preimage handlers
pub fn init_preimage_handlers(window: &MainWindow, app_state: &Arc<Mutex<AppState>>) {
    // Handle generate preimage/hash button
    {
        let app_state = app_state.clone();
        let window_weak = window.as_weak();
        window.on_generate_xh(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            // Note: We're not setting the active page directly anymore
            // This is now handled in the UI through callbacks
            
            thread::spawn(move || {
                let (preimage, hash) = generate_preimage();
                
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.status_message = "Generated preimage/hash pair".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.invoke_update_preimage_hash(
                        SharedString::from(preimage),
                        SharedString::from(hash)
                    );
                    window.set_status_message(SharedString::from("Generated preimage/hash pair"));
                }
            });
        });
    }
} 