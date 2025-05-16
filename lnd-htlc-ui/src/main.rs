mod types;
mod node;
mod wallet;
mod invoice;
mod ui_handlers;
mod utils;

use anyhow::Result;
use slint::SharedString;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use types::AppState;
use wallet::check_litd_config;
use utils::spawn_ui_timer;
use invoice::{create_invoice, create_standard_invoice, pay_invoice, check_invoice, settle_invoice, list_channels, open_channel};

slint::include_modules!();

#[tokio::main]
async fn main() -> Result<()> {
    let app_state = Arc::new(Mutex::new(AppState::default()));
    let window = MainWindow::new()?;
    let window_weak = window.as_weak();

    // Check if litd configuration exists, create if not
    match check_litd_config() {
        Ok(existed) => {
            if !existed {
                // Config was newly created
                if let Some(window) = window_weak.upgrade() {
                    window.set_status_message(SharedString::from(
                        "Created new lit.conf file. Please edit it with secure password before starting litd."
                    ));
                }
            }
        },
        Err(e) => {
            // Could not create/check config
            if let Some(window) = window_weak.upgrade() {
                window.set_status_message(SharedString::from(
                    format!("Could not configure litd: {}", e)
                ));
            }
        }
    }

    // Initialize all handlers and timers
    ui_handlers::init_node_status_handlers(&window, &app_state);
    ui_handlers::init_wallet_handlers(&window, &app_state);
    ui_handlers::init_channel_handlers(&window, &app_state);
    ui_handlers::init_invoice_handlers(&window, &app_state);
    ui_handlers::init_preimage_handlers(&window, &app_state);

    // Set up timers for automatic status checking
    let app_state_clone = app_state.clone();
    let window_weak_clone = window_weak.clone();
    spawn_ui_timer(&window, Duration::from_secs(5), move || {
        ui_handlers::check_node_status_timer(&app_state_clone, &window_weak_clone);
    });

    let app_state_clone = app_state.clone();
    let window_weak_clone = window_weak.clone();
    spawn_ui_timer(&window, Duration::from_secs(5), move || {
        ui_handlers::check_invoice_updates_timer(&app_state_clone, &window_weak_clone);
    });

    window.run()?;
    Ok(())
}
