use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use crate::utils::run_lncli;

pub fn create_invoice(
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

pub fn create_standard_invoice(
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

pub fn pay_invoice(bolt11: &str) -> Result<()> {
    run_lncli(&["payinvoice", "--pay_req", bolt11, "--force"])?;
    Ok(())
}

pub fn check_invoice(hash: &str) -> Result<String> {
    let output = run_lncli(&["lookupinvoice", hash])?;
    Ok(output)
}

pub fn settle_invoice(hash: &str, preimage: &str) -> Result<()> {
    run_lncli(&["settleinvoice", "--preimage", preimage])?;
    Ok(())
}

pub fn list_channels() -> Result<String> {
    run_lncli(&["listchannels"])
}

pub fn open_channel(node_pubkey: &str, amount: i32) -> Result<String> {
    run_lncli(&["openchannel", "--node_key", node_pubkey, "--local_amt", &amount.to_string()])
} 