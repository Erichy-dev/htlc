use anyhow::{anyhow, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use sha2::{Digest, Sha256};
use std::process::Command;
use bincode;
use copypasta::{ClipboardContext, ClipboardProvider};

use crate::{InvoiceData, InvoiceDetails, ListInvoicesResponse};

pub fn list_invoices(db: &sled::Db) -> Result<Vec<InvoiceDetails>> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "listinvoices"])
        .output()?;
    // println!("{}", String::from_utf8_lossy(&output.stdout));

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
                            .map_or(false, |deserialized_struct| {
                                println!("Preimage X: {}", deserialized_struct.preimage_x);
                                println!("Preimage H: {}", deserialized_struct.preimage_h);
                                deserialized_struct.is_own_invoice
                            })
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
                    payment_request: i.payment_request.into(),
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
        .args(["--network", "testnet", "addholdinvoice", &preimage_h, "--amt", &amount, "--memo", &memo])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(json) => {
            if let Some(payment_addr) = json.get("payment_addr").and_then(|v| v.as_str()) {
                if let Some(payment_request) = json.get("payment_request").and_then(|v| v.as_str()) {
                    let invoice_output = Command::new("lncli")
                        .args(["--network", "testnet", "decodepayreq", payment_request])
                        .output()?;

                    let invoice_stdout = String::from_utf8_lossy(&invoice_output.stdout).to_string();
                    let invoice_stderr = String::from_utf8_lossy(&invoice_output.stderr).to_string();
                    println!("{}", invoice_stderr);
                    match serde_json::from_str::<serde_json::Value>(&invoice_stdout) {
                        Ok(json) => {
                            let destination_pubkey = json.get("destination").and_then(|v| v.as_str()).unwrap_or("");
                            let identity_pubkey = db.get(b"identity_pubkey")?.unwrap_or(sled::IVec::from(b""));
                            let identity_pubkey_str = String::from_utf8_lossy(&identity_pubkey).to_string();

                            println!("destination_pubkey: {}", destination_pubkey);
                            println!("identity_pubkey: {}", identity_pubkey_str);

                            let is_own_invoice = destination_pubkey == identity_pubkey_str;

                            let invoice_data_to_save = InvoiceData {
                                preimage_x: preimage_x.clone(),
                                preimage_h: preimage_h.clone(),
                                payment_address: payment_addr.to_string(),
                                r_hash: preimage_h.to_string(),
                                is_own_invoice,
                            };
                            let serialized_invoice_data = bincode::serialize(&invoice_data_to_save)?;
                            db.insert(preimage_h.as_bytes(), serialized_invoice_data)?;
                            
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
                Err(anyhow!("No payment_addr found in response"))
            }
        }
        Err(e) => Err(anyhow!("Failed to parse JSON response: {}", e))
    }
}

pub fn settle_invoice(preimage_h: String, db: &sled::Db) -> Result<()> {
    let preimage_x = match db.get(preimage_h.as_bytes()) {
        Ok(Some(invoice_data_ivec)) => {
            match bincode::deserialize::<InvoiceData>(&invoice_data_ivec) {
                Ok(deserialized_struct) => {
                    println!("Found invoice data in DB. Preimage X: {}, Preimage H: {}", deserialized_struct.preimage_x, deserialized_struct.preimage_h);
                    if deserialized_struct.preimage_x.is_empty() {
                        let err_msg = format!("Preimage X is empty for r_hash {} in DB.", preimage_h);
                        println!("{}", err_msg);
                        return Err(anyhow!(err_msg));
                    }
                    deserialized_struct.preimage_x
                }
                Err(e) => {
                    let err_msg = format!("Failed to deserialize InvoiceData for r_hash {}: {}. Data (lossy UTF-8): {:?}", preimage_h, e, String::from_utf8_lossy(&invoice_data_ivec));
                    println!("{}", err_msg);
                    return Err(anyhow!(err_msg));
                }
            }
        }
        Ok(None) => {
            let err_msg = format!("No invoice data found in DB for r_hash: {}", preimage_h);
            println!("{}", err_msg);
            return Err(anyhow!(err_msg));
        }
        Err(e) => {
            let err_msg = format!("Failed to get invoice data from DB for r_hash {}: {}", preimage_h, e);
            println!("{}", err_msg);
            return Err(anyhow!(err_msg));
        }
    };

    println!("Attempting to settle invoice with r_hash: {}, using resolved preimage_x: {}", preimage_h, preimage_x);

    let output = Command::new("lncli")
        .args(["--network", "testnet", "settleinvoice", &preimage_x])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    println!("{}", stderr);

    if !output.status.success() {
        return Err(anyhow!("Failed to settle invoice: {}", stderr));
    }

    Ok(())
}

pub fn copy_payment_request(payment_request: String) -> Result<()> {
    let mut ctx = match ClipboardContext::new() {
        Ok(ctx) => ctx,
        Err(e) => {
            let err_msg = format!("Failed to initialize clipboard: {}", e);
            println!("{}", err_msg);
            return Err(anyhow!(err_msg));
        }
    };

    match ctx.set_contents(payment_request.clone()) {
        Ok(_) => {
            println!("Copied to clipboard: {}", payment_request);
            Ok(())
        },
        Err(e) => {
            let err_msg = format!("Failed to copy to clipboard: {}", e);
            println!("{}", err_msg); 
            Err(anyhow!(err_msg))
        }
    }
}

pub fn create_standard_invoice(amount: String, memo: String, db: &sled::Db) -> Result<String> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "addinvoice", "--amt", &amount, "--memo", &memo])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    match serde_json::from_str::<serde_json::Value>(&stdout) {
        Ok(json) => {
            if let Some(payment_addr) = json.get("payment_addr").and_then(|v| v.as_str()) {
                if let Some(r_hash) = json.get("r_hash").and_then(|v| v.as_str()) {
                    if let Some(payment_request) = json.get("payment_request").and_then(|v| v.as_str()) {
                        let invoice_output = Command::new("lncli")
                            .args(["--network", "testnet", "decodepayreq", payment_request])
                            .output()?;
    
                        let invoice_stdout = String::from_utf8_lossy(&invoice_output.stdout).to_string();
                        let invoice_stderr = String::from_utf8_lossy(&invoice_output.stderr).to_string();
                        println!("{}", invoice_stderr);
                        
                        match serde_json::from_str::<serde_json::Value>(&invoice_stdout) {
                            Ok(json) => {
                                let destination_pubkey = json.get("destination").and_then(|v| v.as_str()).unwrap_or("");
                                let identity_pubkey = db.get(b"identity_pubkey")?.unwrap_or(sled::IVec::from(b""));
                                let identity_pubkey_str = String::from_utf8_lossy(&identity_pubkey).to_string();
    
                                println!("destination_pubkey: {}", destination_pubkey);
                                println!("identity_pubkey: {}", identity_pubkey_str);
    
                                let is_own_invoice = destination_pubkey == identity_pubkey_str;
    
                                let invoice_data_to_save = InvoiceData {
                                    preimage_x: "".to_string(),
                                    preimage_h: r_hash.to_string(),
                                    payment_address: payment_addr.to_string(),
                                    r_hash: r_hash.to_string(),
                                    is_own_invoice,
                                };
                                let serialized_invoice_data = bincode::serialize(&invoice_data_to_save)?;
                                db.insert(r_hash.as_bytes(), serialized_invoice_data)?;
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