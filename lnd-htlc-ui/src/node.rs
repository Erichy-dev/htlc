use anyhow::{anyhow, Result};
use std::process::Command;
use std::os::windows::process::CommandExt;
use std::process::Stdio;
use std::path::Path;

use crate::wallet::is_wallet_locked;

// Windows-specific flag to hide console window
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn check_node_status() -> Result<(bool, String, bool)> {    
    // Try to check if the node is running using the REST API instead of gRPC
    // First, check if litd is running on its REST port (typically 8443)
    println!("DEBUG: Trying to connect to litd using REST API...");
    
    // Use curl to connect to the REST API (may require passing --insecure for self-signed certs)
    let rest_check = Command::new("curl")
        .arg("--insecure")  // Skip TLS verification
        .arg("--silent")    // Don't show progress
        .arg("-H")          // Add header
        .arg("Accept: application/json")  // Request JSON response
        .arg("-X")          // Specify HTTP method
        .arg("GET")         // Use GET method
        .arg("https://127.0.0.1:8443/v1/system/getinfo")  // Try different endpoint
        .output();
    
    match rest_check {
        Ok(output) => {
            let status = output.status;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            
            println!("DEBUG: REST API response status: {}", status);
            println!("DEBUG: REST API stdout: {}", stdout);
            println!("DEBUG: REST API stderr: {}", stderr);
            
            if status.success() && stdout.contains("\"synced_to_chain\"") {
                // Successfully connected to REST API
                let is_synced = stdout.contains("\"synced_to_chain\":true");
                let sync_status = if is_synced {
                    "Chain synced".to_string()
                } else {
                    "Syncing...".to_string()
                };
                
                // Check if wallet is locked by looking for wallet_unlocked field
                let wallet_locked = !stdout.contains("\"wallet_unlocked\":true");
                
                println!("DEBUG: REST API node status - running: true, sync: {}, wallet_locked: {}", 
                         sync_status, wallet_locked);
                
                return Ok((true, sync_status, wallet_locked));
            }
        },
        Err(e) => {
            println!("DEBUG: Error connecting to REST API: {}", e);
        }
    }
    
    // Fallback to standard lncli method if REST fails
    // Get the path to the TLS certificate and macaroon    
    let home_dir = dirs::home_dir().unwrap_or_default();    
    let lit_dir = home_dir.join("AppData").join("Local").join("Lit");    
    let tls_cert_path = lit_dir.join("tls.cert");    
    let macaroon_path = lit_dir.join("testnet").join("lit.macaroon");        
    
    // First check if the node is running at all    
    let mut cmd = Command::new("lncli");    
    cmd.arg("--network=testnet")       
       .arg("--rpcserver=127.0.0.1:10009")
       .arg("--no-macaroons")  // Skip macaroon authentication
       .arg("--insecure"); // Skip TLS verification
           
    
    // Add TLS cert path if it exists    
    if tls_cert_path.exists() {        
        cmd.arg(format!("--tlscertpath={}", tls_cert_path.to_string_lossy()));    
    }        
    
    cmd.arg("getinfo");        
    
    println!("DEBUG: Running command: lncli --network=testnet --rpcserver=127.0.0.1:10009 --no-macaroons --insecure --tlscertpath={} getinfo",              
             tls_cert_path.to_string_lossy());        
    
    let lncli_check = cmd.output();
      
    // If we couldn't run the command at all, node is offline
    if lncli_check.is_err() {
        println!("DEBUG: lncli command failed - node appears to be offline");
        return Ok((false, "Node is offline".to_string(), false));
    }
    
    let output = lncli_check.unwrap();    
    let stderr = String::from_utf8_lossy(&output.stderr);    
    let stdout = String::from_utf8_lossy(&output.stdout);        
    
    println!("DEBUG: lncli command stderr: {}", stderr);    
    println!("DEBUG: lncli command stdout: {}", stdout);
    
    // Explicitly check for wallet locked message
    let wallet_locked = stderr.contains("wallet locked") || 
                       stderr.contains("wallet not unlocked") ||
                       stderr.contains("wallet state: LOCKED") ||
                       stderr.contains("unlock it to enable full RPC access");
    
    if wallet_locked {
        // If wallet is locked, node is running but wallet needs unlock
        // IMPORTANT: Return TRUE for is_running because the node IS running!
        println!("DEBUG: Detected wallet locked message. Node is running but wallet needs unlock.");
        println!("DEBUG: Wallet lock detection strings in stderr: {}", stderr);
        // Return (true, status, true) to indicate node is running but wallet is locked
        return Ok((true, "Wallet locked".to_string(), true));
    }
    
    if !output.status.success() {
        // If failed but not due to wallet lock, node may have other issues
        println!("DEBUG: Command succeeded but returned error: {}", stderr);
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
    
    println!("DEBUG: Node appears to be running with status: {}", sync_status);
    
    // Double-check wallet lock status with explicit command
    match is_wallet_locked() {
        Ok(is_locked) => {
            if is_locked {
                println!("DEBUG: Secondary wallet lock check confirms wallet is locked");
                return Ok((true, sync_status, true));
            } else {
                println!("DEBUG: Secondary wallet lock check confirms wallet is unlocked");
            }
        },
        Err(e) => {
            println!("DEBUG: Error in secondary wallet lock check: {}", e);
            // Continue anyway, since we already got a successful getinfo response
        }
    }
    
    // Node is running and wallet is unlocked
    println!("DEBUG: Node is running and wallet is unlocked");
    Ok((true, sync_status, false))
}

pub fn start_lightning_node() -> Result<u32> {
    // Get the user's home directory
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not determine user's home directory"))?;
    
    // Check if the lit.conf file exists
    let conf_path = home.join("AppData").join("Local").join("Lit").join("lit.conf");
    if !conf_path.exists() {
        return Err(anyhow!("lit.conf file not found. Please run the app again to create it."));
    }
    
    // Start litd as a hidden background process on Windows
    let child = Command::new("litd")
        .arg("--network=testnet")
        .creation_flags(CREATE_NO_WINDOW) // Windows-specific flag to hide console window
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    
    // Get the process ID for tracking
    let pid = child.id();
    
    // Log the process ID for debugging purposes
    println!("Started litd process with PID: {}", pid);
    
    Ok(pid)
} 