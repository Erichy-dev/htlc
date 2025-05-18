use anyhow::{anyhow, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use sha2::{Digest, Sha256};
use std::process::Command;
use bincode;

use crate::{InvoiceData, InvoiceDetails, ListInvoicesResponse};

pub struct InvoiceMinDetails {
    memo: String,
    r_hash: String,
    value: String,
    state: String,
    creation_date: String,
}

pub fn list_invoices(db: &sled::Db) -> Result<Vec<InvoiceDetails>> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "listinvoices"])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));

    match serde_json::from_str::<ListInvoicesResponse>(&String::from_utf8_lossy(&output.stdout)) {
        Ok(parsed_response) => {
            let slint_invoices_vec: Vec<InvoiceDetails> = parsed_response.invoices.into_iter().map(|i| {
                let formatted_date = match i.creation_date.parse::<i64>() {
                    Ok(timestamp_seconds) => {
                        // Ensure NaiveDateTime::from_timestamp_opt is used for safety
                        NaiveDateTime::from_timestamp_opt(timestamp_seconds, 0)
                            .map(|naive_dt| DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc).format("%Y-%m-%d %H:%M:%S UTC").to_string())
                            .unwrap_or_else(|| {
                                println!("Warning: Failed to format timestamp {} for r_hash {}", timestamp_seconds, i.r_hash);
                                i.creation_date.clone() // Fallback to original string
                            })
                    }
                    Err(e) => {
                        println!("Warning: Failed to parse creation_date '{}' as i64 for r_hash {}: {}", i.creation_date, i.r_hash, e);
                        i.creation_date.clone() // Fallback to original string if timestamp is not a valid i64
                    }
                };

                let is_own_invoice = match db.get(i.r_hash.as_bytes()) {
                    Ok(Some(invoice_data)) => {
                        bincode::deserialize::<InvoiceData>(&invoice_data)
                            .map_or(false, |deserialized_struct| deserialized_struct.is_own_invoice)
                    }
                    _ => false,
                };

                InvoiceDetails {
                    memo: i.memo.into(),
                    r_hash: i.r_hash.into(),
                    value: i.value.into(),
                    state: i.state.into(),
                    creation_date: formatted_date.into(),
                    is_own_invoice,
                }
            }).collect();

            Ok(slint_invoices_vec)
        }
        Err(e) => {
            let error_msg = format!("Error parsing invoices JSON: {}", e);
            println!("{}", error_msg);
            Err(anyhow!("Error parsing invoices JSON: {}", e))
        }
    }
} 

pub fn create_invoice(preimage_x: String, preimage_h: String, amount: String, memo: String, db: &sled::Db) -> Result<String> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "addholdinvoice", &preimage_x, "--amt", &amount, "--memo", &memo])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(json) => {
            if let Some(payment_addr) = json.get("payment_addr").and_then(|v| v.as_str()) {
                if let Some(r_hash) = json.get("r_hash").and_then(|v| v.as_str()) {
                    if let Some(payment_request) = json.get("payment_request").and_then(|v| v.as_str()) {
                        let invoice_output = Command::new("lncli")
                            .args(["--network", "decodepayreq", payment_request])
                            .output()?;
                        let invoice_stdout = String::from_utf8_lossy(&invoice_output.stdout).to_string();
                        match serde_json::from_str::<serde_json::Value>(&invoice_stdout) {
                            Ok(json) => {
                                let destination_pubkey = json.get("destination").and_then(|v| v.as_str()).unwrap_or("");
                                let identity_pubkey = db.get(b"identity_pubkey")?.unwrap_or(sled::IVec::from(b""));
                                let identity_pubkey_str = String::from_utf8_lossy(&identity_pubkey).to_string();
    
                                let is_own_invoice = destination_pubkey == identity_pubkey_str;
    
                                let invoice_data_to_save = InvoiceData {
                                    preimage_x: preimage_x.clone(),
                                    preimage_h: preimage_h.clone(),
                                    payment_address: payment_addr.to_string(),
                                    r_hash: r_hash.to_string(),
                                    is_own_invoice,
                                };
                                let serialized_invoice_data = bincode::serialize(&invoice_data_to_save)?;
                                db.insert(preimage_x.as_bytes(), serialized_invoice_data)?;
                               
                                Ok(payment_addr.to_string())
                            }
                            Err(e) => {
                                println!("Failed to parse JSON response: {}", e);
                                Err(anyhow!("Failed to parse JSON response: {}", e))
                            }
                        }
                    } else {
                        Err(anyhow!("No payment_request found in response"))
                    }
                } else {
                    Err(anyhow!("No r_hash found in response"))
                }
            } else {
                Err(anyhow!("No payment_addr found in response"))
            }
        }
        Err(e) => Err(anyhow!("Failed to parse JSON response: {}", e))
    }
}