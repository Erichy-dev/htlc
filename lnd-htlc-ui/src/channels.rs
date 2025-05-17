use anyhow::{anyhow, Result};
use std::process::Command;
use serde_json::Value;

pub fn connect_to_peer(pubkey: &str, host: &str, port: u16) -> Result<String> {
    let addr = format!("{}@{}:{}", pubkey, host, port);
    
    println!("Attempting to connect to peer: {}", addr);
    
    let output = Command::new("lncli")
        .args(["--network", "testnet", "connect", &addr])
        .output()?;
    
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Successfully connected to peer: {}", stdout);
        Ok(stdout.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("Failed to connect to peer: {}", stderr);
        Err(anyhow!("Failed to connect: {}", stderr))
    }
}

pub fn list_channels() -> Result<String> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "listchannels"])
        .output()?;
        
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("Failed to list channels: {}", stderr))
    }
}

pub fn list_peers() -> Result<Vec<String>> {
    println!("Listing connected peers...");
    
    let output = Command::new("lncli")
        .args(["--network", "testnet", "listpeers"])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to list peers: {}", stderr));
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Peers list retrieved successfully");
    
    // Parse JSON to extract peer public keys
    let json: Value = serde_json::from_str(&stdout)
        .map_err(|e| anyhow!("Failed to parse JSON: {}", e))?;
    
    // Extract public keys
    let peers = json["peers"].as_array()
        .ok_or_else(|| anyhow!("No peers array found in response"))?;
    
    let mut pub_keys = Vec::new();
    for peer in peers {
        if let Some(pub_key) = peer["pub_key"].as_str() {
            pub_keys.push(pub_key.to_string());
        }
    }
    
    if pub_keys.is_empty() {
        return Err(anyhow!("No peers found"));
    }
    
    Ok(pub_keys)
}

pub fn auto_open_channel(amount: u32) -> Result<String> {
    // Get list of peers
    let peers = list_peers()?;
    
    // Choose the first peer
    let peer_pubkey = &peers[0];
    println!("Selected peer with pubkey: {}", peer_pubkey);
    
    // Open channel with selected peer
    open_channel(peer_pubkey, amount)
}

pub fn open_channel(pub_key: &str, amount: u32) -> Result<String> {
    println!("Opening channel with {} for {} sats", pub_key, amount);
    
    let output = Command::new("lncli")
        .args([
            "--network", "testnet",
            "openchannel",
            pub_key,
            &amount.to_string()
        ])
        .output()?;
    
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Channel open success: {}", stdout);
        Ok(stdout.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("Channel open failed: {}", stderr);
        Err(anyhow!("Failed to open channel: {}", stderr))
    }
}
