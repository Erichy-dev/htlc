use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use std::process::Command;

pub fn list_invoices() -> Result<String> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "listinvoices"])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
} 

pub fn create_invoice(preimage: String, amount: String, memo: String) -> Result<String> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "addholdinvoice", &preimage, "--amt", &amount, "--memo", &memo])
        .output()?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}