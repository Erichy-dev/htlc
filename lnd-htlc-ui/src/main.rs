use anyhow::{anyhow, Result};
use rand::RngCore;
use sha2::{Digest, Sha256};
use slint::{SharedString, Timer, TimerMode};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

slint::include_modules!();

#[derive(Debug, Clone)]
struct AppState {
    invoices: Vec<Invoice>,
    status_message: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            invoices: Vec::new(),
            status_message: String::new(),
        }
    }
}

fn generate_preimage() -> (String, String) {
    let mut rng = rand::thread_rng();
    let mut preimage = vec![0u8; 32];
    rng.fill_bytes(&mut preimage);
    
    let preimage_hex = hex::encode(&preimage);
    let hash = Sha256::digest(&preimage);
    let hash_hex = hex::encode(hash);
    
    (preimage_hex, hash_hex)
}

fn run_lncli(args: &[&str]) -> Result<String> {
    let output = Command::new("lncli")
        .args(["--network", "testnet"])
        .args(args)
        .output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "lncli command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8(output.stdout)?)
}

fn create_invoice(
    preimage: String,
    amount_str: String,
    memo: String,
) -> Result<(String, String, i32)> {
    // Parse amount string to i64
    let amount = amount_str.parse::<i32>().map_err(|_| anyhow!("Invalid amount"))?;

    let hash = {
        let preimage_bytes = hex::decode(&preimage)?;
        let hash = Sha256::digest(&preimage_bytes);
        hex::encode(hash)
    };

    let output = run_lncli(&[
        "addholdinvoice",
        &hash,
        "--amt",
        &amount.to_string(),
        "--memo",
        &memo,
    ])?;

    Ok((output.trim().to_string(), hash, amount))
}

fn check_invoice(hash: &str) -> Result<String> {
    let output = run_lncli(&["lookupinvoice", hash])?;
    Ok(output)
}

fn settle_invoice(hash: &str, preimage: &str) -> Result<()> {
    run_lncli(&["settleinvoice", "--preimage", preimage])?;
    Ok(())
}

fn spawn_ui_timer<F>(window: &MainWindow, interval: Duration, callback: F)
where
    F: Fn() + 'static,
{
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, interval, move || {
        callback();
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let app_state = Arc::new(Mutex::new(AppState::default()));
    let window = MainWindow::new()?;
    let window_weak = window.as_weak();

    // Handle invoice creation
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        window.on_create_invoice(move |preimage, amount, memo| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                match create_invoice(preimage.to_string(), amount.to_string(), memo.to_string()) {
                    Ok((bolt11, hash, amount)) => {
                        if let Ok(mut state) = app_state.lock() {
                            state.invoices.push(Invoice {
                                bolt11: SharedString::from(bolt11),
                                hash: SharedString::from(hash.clone()),
                                preimage: SharedString::from(preimage.to_string()),
                                amount,
                                memo: SharedString::from(memo.to_string()),
                                state: SharedString::from("PENDING"),
                            });
                            state.status_message = format!("Created invoice for {} sats", amount);
                            
                            if let Some(window) = window_weak.upgrade() {
                                window.set_invoices(slint::ModelRc::new(slint::VecModel::from(
                                    state.invoices.clone(),
                                )));
                                window.set_status_message(state.status_message.clone().into());
                            }
                        }
                    }
                    Err(e) => {
                        if let Ok(mut state) = app_state.lock() {
                            state.status_message = format!("Error: {}", e);
                            if let Some(window) = window_weak.upgrade() {
                                window.set_status_message(state.status_message.clone().into());
                            }
                        }
                    }
                }
            });
        });
    }

    // Handle invoice settlement
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        window.on_settle_invoice(move |hash, preimage| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                match settle_invoice(&hash, &preimage) {
                    Ok(()) => {
                        if let Ok(mut state) = app_state.lock() {
                            if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
                                invoice.state = SharedString::from("SETTLED");
                            }
                            state.status_message = "Invoice settled successfully!".to_string();
                            
                            if let Some(window) = window_weak.upgrade() {
                                window.set_invoices(slint::ModelRc::new(slint::VecModel::from(
                                    state.invoices.clone(),
                                )));
                                window.set_status_message(state.status_message.clone().into());
                            }
                        }
                    }
                    Err(e) => {
                        if let Ok(mut state) = app_state.lock() {
                            state.status_message = format!("Error: {}", e);
                            if let Some(window) = window_weak.upgrade() {
                                window.set_status_message(state.status_message.clone().into());
                            }
                        }
                    }
                }
            });
        });
    }

    // Set up invoice checking timer
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        spawn_ui_timer(&window, Duration::from_secs(5), move || {
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
                if let Ok(mut state) = app_state.lock() {
                    for (hash, new_state) in updates {
                        if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
                            invoice.state = SharedString::from(new_state);
                        }
                    }
                    
                    state.status_message = "Updated invoice states".to_string();
                    
                    if let Some(window) = window_weak.upgrade() {
                        window.set_invoices(slint::ModelRc::new(slint::VecModel::from(
                            state.invoices.clone(),
                        )));
                        window.set_status_message(state.status_message.clone().into());
                    }
                }
            }
        });
    }

    window.run()?;
    Ok(())
}
