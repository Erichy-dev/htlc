use anyhow::{anyhow, Result};
use rand::RngCore;
use sha2::{Digest, Sha256};
use slint::SharedString;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;

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

async fn create_invoice(
    app_state: Arc<Mutex<AppState>>,
    preimage: String,
    amount: i64,
    memo: String,
) -> Result<()> {
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

    let mut state = app_state.lock().await;
    state.invoices.push(Invoice {
        bolt11: SharedString::from(output.trim()),
        hash: SharedString::from(hash),
        preimage: SharedString::from(preimage),
        amount,
        memo: SharedString::from(memo),
        state: SharedString::from("PENDING"),
    });
    state.status_message = format!("Created invoice for {} sats", amount);

    Ok(())
}

async fn check_invoice(app_state: Arc<Mutex<AppState>>, hash: String) -> Result<()> {
    let output = run_lncli(&["lookupinvoice", &hash])?;
    
    let mut state = app_state.lock().await;
    if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
        if output.contains("\"state\": \"ACCEPTED\"") {
            invoice.state = SharedString::from("ACCEPTED");
            state.status_message = "Payment received and held!".to_string();
        } else if output.contains("\"state\": \"SETTLED\"") {
            invoice.state = SharedString::from("SETTLED");
            state.status_message = "Payment settled!".to_string();
        }
    }

    Ok(())
}

async fn settle_invoice(app_state: Arc<Mutex<AppState>>, hash: String, preimage: String) -> Result<()> {
    run_lncli(&["settleinvoice", "--preimage", &preimage])?;

    let mut state = app_state.lock().await;
    if let Some(invoice) = state.invoices.iter_mut().find(|i| i.hash == hash) {
        invoice.state = SharedString::from("SETTLED");
        state.status_message = "Invoice settled successfully!".to_string();
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let app_state = Arc::new(Mutex::new(AppState::default()));
    let window = MainWindow::new()?;
    
    // Set up invoice checking interval
    let check_state = app_state.clone();
    tokio::spawn(async move {
        loop {
            let state = check_state.lock().await;
            for invoice in &state.invoices {
                if invoice.state == "PENDING" {
                    drop(state);
                    let _ = check_invoice(check_state.clone(), invoice.hash.to_string()).await;
                    break;
                }
            }
            drop(state);
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    });

    // Handle invoice creation
    let create_state = app_state.clone();
    window.on_create_invoice(move |preimage, amount, memo| {
        let state = create_state.clone();
        tokio::spawn(async move {
            if let Err(e) = create_invoice(state.clone(), preimage.to_string(), amount as i64, memo.to_string()).await {
                let mut state = state.lock().await;
                state.status_message = format!("Error: {}", e);
            }
        });
    });

    // Handle invoice settlement
    let settle_state = app_state.clone();
    window.on_settle_invoice(move |hash, preimage| {
        let state = settle_state.clone();
        tokio::spawn(async move {
            if let Err(e) = settle_invoice(state.clone(), hash.to_string(), preimage.to_string()).await {
                let mut state = state.lock().await;
                state.status_message = format!("Error: {}", e);
            }
        });
    });

    // Update UI state
    let weak_window = window.as_weak();
    tokio::spawn(async move {
        loop {
            if let Some(window) = weak_window.upgrade() {
                let state = app_state.lock().await;
                window.set_invoices(slint::ModelRc::new(slint::VecModel::from(
                    state.invoices.clone(),
                )));
                window.set_status_message(state.status_message.clone().into());
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    });

    window.run()?;
    Ok(())
}
