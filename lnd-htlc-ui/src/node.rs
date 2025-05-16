use anyhow::{anyhow, Result};
use std::process::Command;

use crate::wallet::is_wallet_locked;

pub fn check_node_status() -> Result<(bool, String, bool)> {
    // First check if the node is running at all
    let lncli_check = Command::new("lncli")
        .arg("--network=testnet")
        .arg("--rpcserver=127.0.0.1:10009")
        .arg("getinfo")
        .output();
    
    // If we couldn't run the command at all, node is offline
    if lncli_check.is_err() {
        return Ok((false, "Node is offline".to_string(), false));
    }
    
    let output = lncli_check.unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Explicitly check for wallet locked message
    let wallet_locked = stderr.contains("wallet locked") || 
                       stderr.contains("wallet not unlocked") ||
                       stderr.contains("wallet state: LOCKED") ||
                       stderr.contains("unlock it to enable full RPC access");
    
    if wallet_locked {
        // If wallet is locked, node is running but wallet needs unlock
        println!("Detected wallet is locked. stderr: {}", stderr);
        return Ok((true, "Wallet locked".to_string(), true));
    }
    
    if !output.status.success() {
        // If failed but not due to wallet lock, node may have other issues
        println!("Node not responding properly. stderr: {}", stderr);
        return Ok((false, "Node is not responding".to_string(), false));
    }
    
    // Successfully got info, check sync status
    let sync_status = if stdout.contains("\"synced_to_chain\":true") {
        "Chain synced".to_string()
    } else if stdout.contains("\"synced_to_chain\":false") {
        "Syncing...".to_string()
    } else {
        "Unknown".to_string()
    };
    
    // Double-check wallet lock status with explicit command
    match is_wallet_locked() {
        Ok(is_locked) => {
            if is_locked {
                println!("Wallet explicitly checked and confirmed as locked");
                return Ok((true, sync_status, true));
            }
        },
        Err(e) => {
            println!("Error checking wallet lock status: {}", e);
            // Continue anyway, since we already got a successful getinfo response
        }
    }
    
    // Node is running and wallet is unlocked
    Ok((true, sync_status, false))
}

pub fn start_lightning_node() -> Result<()> {
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
    command.args(["-c", "gnome-terminal -- bash -c 'litd --network=testnet; read'"]);
    
    // Execute the command
    command.spawn()?;
    
    Ok(())
} 