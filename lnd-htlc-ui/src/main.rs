use anyhow::{anyhow, Result};
use rand::RngCore;
use sha2::{Digest, Sha256};
use slint::{SharedString, Timer, TimerMode};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::path::Path;
use std::fs;
use std::io::Write;

slint::include_modules!();

#[derive(Debug, Clone)]
struct AppState {
    invoices: Vec<Invoice>,
    status_message: String,
    preimage_output: String,
    hash_output: String,
    node_is_running: bool,
    node_sync_status: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            invoices: Vec::new(),
            status_message: String::new(),
            preimage_output: String::new(),
            hash_output: String::new(),
            node_is_running: false,
            node_sync_status: "Unknown".to_string(),
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
    // Build command with custom RPC server settings
    let mut command = Command::new("lncli");
    
    // Add network flag
    command.arg("--network=testnet");
    
    // Add custom RPC server flag - adjust this based on your litd configuration
    // Use this if your LND RPC server is running on a non-default port
    command.arg("--rpcserver=127.0.0.1:10009");
    
    // Add the rest of the arguments
    command.args(args);
    
    let output = command.output()?;

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

fn check_node_status() -> Result<(bool, String)> {
    // Build command with custom RPC server settings
    let mut command = Command::new("lncli");
    
    // Add network flag
    command.arg("--network=testnet");
    
    // Add custom RPC server flag - adjust this based on your litd configuration
    // Use this if your LND RPC server is running on a non-default port
    command.arg("--rpcserver=127.0.0.1:10009");
    
    // Add getinfo command
    command.arg("getinfo");
    
    let output = command.output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                return Ok((false, "Node is not responding".to_string()));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Extract sync status from JSON output
            let sync_status = if stdout.contains("\"synced_to_chain\":true") {
                "Chain synced".to_string()
            } else if stdout.contains("\"synced_to_chain\":false") {
                "Syncing...".to_string()
            } else {
                "Unknown".to_string()
            };

            Ok((true, sync_status))
        },
        Err(_) => Ok((false, "Node is offline".to_string())),
    }
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

fn check_litd_config() -> Result<bool> {
    // Get user's home directory
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not determine user's home directory"))?;

    // Check if the .lit directory exists, create if not
    let lit_dir = home.join(".lit");
    if !lit_dir.exists() {
        fs::create_dir_all(&lit_dir)?;
    }

    // Check if the lit.conf file exists
    let conf_path = lit_dir.join("lit.conf");
    if !conf_path.exists() {
        // Create a default config file
        let mut file = fs::File::create(&conf_path)?;
        let default_config = r#"httpslisten=0.0.0.0:8443
uipassword=password_change_me
lnd-mode=integrated
lnd.bitcoin.active=1
lnd.bitcoin.testnet=1
lnd.bitcoin.node=neutrino
lnd.feeurl=https://nodes.lightning.computer/fees/v1/btc-fee-estimates.json
lnd.protocol.option-scid-alias=true
lnd.protocol.zero-conf=true
"#;
        file.write_all(default_config.as_bytes())?;
        
        return Ok(false); // Indicates we created a new config
    }

    Ok(true) // Config already existed
}

fn start_lightning_node() -> Result<()> {
    // Get the user's home directory
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not determine user's home directory"))?;
    
    // Check if the lit.conf file exists
    let conf_path = home.join(".lit").join("lit.conf");
    if !conf_path.exists() {
        return Err(anyhow!("lit.conf file not found. Please run the app again to create it."));
    }
    
    // Use a different command based on the OS
    #[cfg(target_os = "windows")]
    let mut command = Command::new("cmd");
    #[cfg(target_os = "windows")]
    command.args(["/c", "start", "cmd", "/k", "litd", "--network=testnet"]);
    
    #[cfg(not(target_os = "windows"))]
    let mut command = Command::new("sh");
    #[cfg(not(target_os = "windows"))]
    command.args(["-c", "litd --network=testnet &"]);
    
    // Execute the command
    command.spawn()?;
    
    Ok(())
}

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

    // Check node status at startup
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        
        std::thread::spawn(move || {
            match check_node_status() {
                Ok((is_running, sync_status)) => {
                    {
                        if let Ok(mut state) = app_state.lock() {
                            state.node_is_running = is_running;
                            state.node_sync_status = sync_status.clone();
                            if !is_running {
                                state.status_message = "Lightning node (lnd) is not running. Please start litd using: litd --network testnet".to_string();
                            }
                        }
                    }
                    
                    if let Some(window) = window_weak.upgrade() {
                        window.set_node_is_running(is_running);
                        window.set_node_sync_status(SharedString::from(sync_status));
                        if !is_running {
                            window.set_status_message(SharedString::from(
                                "Lightning node (lnd) is not running. Please start litd using: litd --network testnet"
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
        let window_weak = window_weak.clone();
        window.on_check_node_status(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                match check_node_status() {
                    Ok((is_running, sync_status)) => {
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.node_is_running = is_running;
                                state.node_sync_status = sync_status.clone();
                                state.status_message = "Node status updated".to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_node_is_running(is_running);
                            window.set_node_sync_status(SharedString::from(sync_status));
                            window.set_status_message(SharedString::from("Node status updated"));
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
        let window_weak = window_weak.clone();
        window.on_start_node(move || {
            let app_state = app_state.clone();
            let window_weak = window_weak.clone();
            
            std::thread::spawn(move || {
                match start_lightning_node() {
                    Ok(()) => {
                        let status_msg = "Starting Lightning node... Please wait a few moments for it to initialize.";
                        {
                            if let Ok(mut state) = app_state.lock() {
                                state.status_message = status_msg.to_string();
                            }
                        }
                        
                        if let Some(window) = window_weak.upgrade() {
                            window.set_status_message(SharedString::from(status_msg));
                        }
                        
                        // Wait a bit and then check the status
                        std::thread::sleep(Duration::from_secs(5));
                        
                        match check_node_status() {
                            Ok((is_running, sync_status)) => {
                                {
                                    if let Ok(mut state) = app_state.lock() {
                                        state.node_is_running = is_running;
                                        state.node_sync_status = sync_status.clone();
                                        if is_running {
                                            state.status_message = "Lightning node started successfully".to_string();
                                        } else {
                                            state.status_message = "Lightning node still starting. Check status again in a moment.".to_string();
                                        }
                                    }
                                }
                                
                                if let Some(window) = window_weak.upgrade() {
                                    window.set_node_is_running(is_running);
                                    window.set_node_sync_status(SharedString::from(sync_status));
                                    if is_running {
                                        window.set_status_message(SharedString::from("Lightning node started successfully"));
                                    } else {
                                        window.set_status_message(SharedString::from("Lightning node still starting. Check status again in a moment."));
                                    }
                                }
                            },
                            Err(e) => {
                                let error_msg = format!("Error checking node after start: {}", e);
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

    // Set up node status checking timer
    {
        let app_state = app_state.clone();
        let window_weak = window_weak.clone();
        spawn_ui_timer(&window, Duration::from_secs(30), move || {
            match check_node_status() {
                Ok((is_running, sync_status)) => {
                    {
                        if let Ok(mut state) = app_state.lock() {
                            // Only update if status changed
                            if state.node_is_running != is_running || 
                               state.node_sync_status != sync_status {
                                state.node_is_running = is_running;
                                state.node_sync_status = sync_status.clone();
                            }
                        }
                    }
                    
                    if let Some(window) = window_weak.upgrade() {
                        window.set_node_is_running(is_running);
                        window.set_node_sync_status(SharedString::from(sync_status));
                    }
                },
                Err(_) => {
                    {
                        if let Ok(mut state) = app_state.lock() {
                            if state.node_is_running {
                                state.node_is_running = false;
                                state.node_sync_status = "Offline".to_string();
                            }
                        }
                    }
                    
                    if let Some(window) = window_weak.upgrade() {
                        window.set_node_is_running(false);
                        window.set_node_sync_status(SharedString::from("Offline"));
                    }
                }
            }
        });
    }

    window.run()?;
    Ok(())
}
