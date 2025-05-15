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
    preimage_output: String,
    hash_output: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            invoices: Vec::new(),
            status_message: String::new(),
            preimage_output: String::new(),
            hash_output: String::new(),
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

fn create_standard_invoice(
    memo: String,
    amount: i32,
) -> Result<(String, String)> {
    let output = run_lncli(&[
        "addinvoice",
        "--amt",
        &amount.to_string(),
        "--memo",
        &memo,
    ])?;

    // Extract r_hash and payment_request from JSON response
    let r_hash = ""; // Parse from output
    let payment_request = ""; // Parse from output
    
    // Simple parsing (should use serde_json in production)
    let output_str = output.as_str();
    let r_hash = if let Some(start) = output_str.find("\"r_hash\":") {
        let start = start + 10;
        if let Some(end) = output_str[start..].find("\"") {
            output_str[start..(start + end)].trim().trim_matches('"').to_string()
        } else {
            return Err(anyhow!("Failed to parse r_hash"));
        }
    } else {
        return Err(anyhow!("r_hash not found in response"));
    };
    
    let payment_request = if let Some(start) = output_str.find("\"payment_request\":") {
        let start = start + 18;
        if let Some(end) = output_str[start..].find("\"") {
            output_str[start..(start + end)].trim().trim_matches('"').to_string()
        } else {
            return Err(anyhow!("Failed to parse payment_request"));
        }
    } else {
        return Err(anyhow!("payment_request not found in response"));
    };

    Ok((payment_request, r_hash))
}

fn pay_invoice(bolt11: &str) -> Result<()> {
    run_lncli(&["payinvoice", "--pay_req", bolt11, "--force"])?;
    Ok(())
}

fn check_invoice(hash: &str) -> Result<String> {
    let output = run_lncli(&["lookupinvoice", hash])?;
    Ok(output)
}

fn list_channels() -> Result<String> {
    run_lncli(&["listchannels"])
}

fn open_channel(node_pubkey: &str, amount: i32) -> Result<String> {
    run_lncli(&["openchannel", "--node_key", node_pubkey, "--local_amt", &amount.to_string()])
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

    // Handle manage channels
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        window.on_manage_channels(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                match list_channels() {
                    Ok(channels_json) => {
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = "Channels retrieved successfully".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from("Channels retrieved successfully"));
                            // Here we would parse and display the channels data
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
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
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        window.on_create_channel(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            // This just updates the UI to show the create channel page
            {
                if let Ok(mut state) = app_state.lock() {
                    state.status_message = "Enter node public key and amount to open a channel".to_string();
                }
            }
            
            if let Some(window) = window_weak.upgrade() {
                window.set_status_message(SharedString::from("Enter node public key and amount to open a channel"));
            }
        });
    }

    // Handle generate x/h
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        window.on_generate_xh(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                let (preimage, hash) = generate_preimage();
                
                {
                    if let Ok(mut state) = app_state.lock() {
                        state.preimage_output = preimage.clone();
                        state.hash_output = hash.clone();
                        state.status_message = "Generated new preimage and hash".to_string();
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    // Update the text content via a callback that we'll implement in the UI
                    window.invoke_update_preimage_hash(
                        SharedString::from(preimage), 
                        SharedString::from(hash)
                    );
                    window.set_status_message(SharedString::from("Generated new preimage and hash"));
                }
            });
        });
    }

    // Handle create custom invoice
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        window.on_create_custom_invoice(move |preimage, amount, memo| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                match create_invoice(preimage.to_string(), amount.to_string(), memo.to_string()) {
                    Ok((bolt11, hash, amount)) => {
                        let new_invoice = Invoice {
                            bolt11: SharedString::from(bolt11),
                            hash: SharedString::from(hash.clone()),
                            preimage: SharedString::from(preimage.to_string()),
                            amount,
                            memo: SharedString::from(memo.to_string()),
                            state: SharedString::from("PENDING"),
                        };
                        let status_message = format!("Created invoice for {} sats", amount);
                        
                        let invoices_clone;
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.invoices.push(new_invoice);
                                state.status_message = status_message.clone();
                                invoices_clone = state.invoices.clone();
                            } else {
                                return;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_invoices(slint::ModelRc::new(slint::VecModel::from(
                                invoices_clone,
                            )));
                            window.set_status_message(SharedString::from(status_message));
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
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
    
    // Handle pay custom invoice
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        window.on_pay_custom_invoice(move |bolt11| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                match pay_invoice(&bolt11) {
                    Ok(()) => {
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = "Payment sent successfully!".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from("Payment sent successfully!"));
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
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

    // Handle claim custom invoice
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        window.on_claim_custom_invoice(move |hash, preimage| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                match settle_invoice(&hash, &preimage) {
                    Ok(()) => {
                        let invoices_clone;
                        {
                            if let Ok(mut state) = app_state.lock() {
                                if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
                                    invoice.state = SharedString::from("SETTLED");
                                }
                                state.status_message = "Invoice settled successfully!".to_string();
                                invoices_clone = state.invoices.clone();
                            } else {
                                return;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_invoices(slint::ModelRc::new(slint::VecModel::from(
                                invoices_clone,
                            )));
                            window.set_status_message(SharedString::from("Invoice settled successfully!"));
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
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
    
    // Handle create standard invoice
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        window.on_create_standard_invoice(move |memo, amount| {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                match create_standard_invoice(memo.to_string(), amount) {
                    Ok((bolt11, hash)) => {
                        let new_invoice = Invoice {
                            bolt11: SharedString::from(bolt11),
                            hash: SharedString::from(hash.clone()),
                            preimage: SharedString::from(""),  // Standard invoices don't expose preimage
                            amount,
                            memo: SharedString::from(memo.to_string()),
                            state: SharedString::from("PENDING"),
                        };
                        let status_message = format!("Created standard invoice for {} sats", amount);
                        let invoices_clone;
                        
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.invoices.push(new_invoice);
                                state.status_message = status_message.clone();
                                invoices_clone = state.invoices.clone();
                            } else {
                                return;
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_invoices(slint::ModelRc::new(slint::VecModel::from(
                                invoices_clone,
                            )));
                            window.set_status_message(SharedString::from(status_message));
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Error: {}", e);
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
                let invoices_clone;
                {
                    if let Ok(mut state) = app_state.lock() {
                        for (hash, new_state) in updates {
                            if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
                                invoice.state = SharedString::from(new_state);
                            }
                        }
                        
                        state.status_message = "Updated invoice states".to_string();
                        invoices_clone = state.invoices.clone();
                    } else {
                        return;
                    }
                }
                
                if let Some(window) = window_weak.upgrade() {
                    window.set_invoices(slint::ModelRc::new(slint::VecModel::from(
                        invoices_clone,
                    )));
                    window.set_status_message(SharedString::from("Updated invoice states"));
                }
            }
        });
    }

    window.run()?;
    Ok(())
}
