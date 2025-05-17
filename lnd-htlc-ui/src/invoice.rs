use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use std::process::Command;
use bincode;

use crate::InvoiceData;

pub fn list_invoices() -> Result<String> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "listinvoices"])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
} 

pub fn create_invoice(preimage: String, amount: String, memo: String, db: &sled::Db) -> Result<String> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "addholdinvoice", &preimage, "--amt", &amount, "--memo", &memo])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(json) => {
            if let Some(payment_addr) = json.get("payment_addr").and_then(|v| v.as_str()) {
                let invoice_data_to_save = InvoiceData {
                    preimage_x: preimage.clone(),
                    preimage_h: preimage.clone(),
                    payment_address: payment_addr.to_string(),
                };
                let serialized_invoice_data = bincode::serialize(&invoice_data_to_save)?;
                db.insert(preimage.as_bytes(), serialized_invoice_data)?;
                Ok(payment_addr.to_string())
            } else {
                Err(anyhow!("No payment_addr found in response"))
            }
        }
        Err(e) => Err(anyhow!("Failed to parse JSON response: {}", e))
    }
}