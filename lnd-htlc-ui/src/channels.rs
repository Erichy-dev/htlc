use anyhow::{anyhow, Result};
use std::process::Command;

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
