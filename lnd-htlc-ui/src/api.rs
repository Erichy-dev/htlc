use anyhow::{anyhow, Result};
use std::path::Path;
use std::fs;
use crate::types::LndConnection;

// For now, we'll use a simplified version without direct tonic_lnd usage
// This will allow us to test the litd background process functionality

/// Get basic info from the Lightning Network node using API
pub async fn get_node_info(config: &LndConnection) -> Result<(bool, String, bool)> {
    // Try to connect to the REST API first
    println!("DEBUG: Trying to connect to REST API...");
    
    // Use curl to connect to the REST API
    let rest_check = std::process::Command::new("curl")
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
    
    // Fallback to the original approach
    // Check if cert and macaroon exist to simulate a basic connection test
    let cert_path = &config.cert_path;
    let macaroon_path = &config.macaroon_path;
    
    let cert_exists = Path::new(cert_path).exists();
    let macaroon_exists = Path::new(macaroon_path).exists();
    
    if !cert_exists {
        return Err(anyhow!("TLS certificate not found at: {}", cert_path));
    }
    
    if !macaroon_exists {
        return Err(anyhow!("Macaroon file not found at: {}", macaroon_path));
    }
    
    // For now just simulate a successful connection
    // In a full implementation, we would connect to the gRPC API
    println!("API test: found cert at {} and macaroon at {}", cert_path, macaroon_path);
    
    // Fall back to lncli command for actual node status
    // In the future this would be replaced with direct gRPC calls
    match crate::node::check_node_status() {
        Ok(status) => Ok(status),
        Err(e) => Err(anyhow!("Error checking node status: {}", e))
    }
}

/// Unlock the wallet using API - temporarily just calls the command line version
pub async fn unlock_wallet(config: &LndConnection, password: &str) -> Result<bool> {
    // Just use the existing function for now
    crate::wallet::unlock_wallet(password)
}

/// List channels using API - temporarily just calls the command line version
pub async fn list_channels(config: &LndConnection) -> Result<String> {
    // Just use the existing function for now
    crate::invoice::list_channels()
}

/// Get API connection settings from lit.conf
pub fn get_connection_from_config() -> Result<LndConnection> {
    // Look for lit files in AppData directory on Windows
    let lit_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not determine user's home directory"))?
        .join("AppData")
        .join("Local")
        .join("Lit");
    
    // Check if the lit.conf file exists
    let conf_path = lit_dir.join("lit.conf");
    if !conf_path.exists() {
        return Err(anyhow!("lit.conf file not found at {}. Please run litd to create it.", conf_path.display()));
    }
    
    // Read and parse the config file (simplified - a real implementation would be more robust)
    let content = fs::read_to_string(conf_path)?;
    
    // Create a default connection and update it based on config
    let mut connection = LndConnection::default();
    
    // Parse the config file (very simplified - real code would use a proper parser)
    for line in content.lines() {
        if line.starts_with("rpclisten=") {
            if let Some(listen_addr) = line.strip_prefix("rpclisten=") {
                if let Some((host, port_str)) = listen_addr.split_once(':') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        connection.host = host.to_string();
                        connection.port = port;
                    }
                }
            }
        }
    }
    
    // Update paths based on the Windows AppData location
    connection.cert_path = lit_dir.join("tls.cert").to_string_lossy().to_string();
    
    // For Lightning Terminal, look for lit.macaroon in the testnet directory
    let testnet_dir = lit_dir.join("testnet");
    let lit_macaroon_path = testnet_dir.join("lit.macaroon");
    
    if lit_macaroon_path.exists() {
        // Use lit.macaroon if it exists
        connection.macaroon_path = lit_macaroon_path.to_string_lossy().to_string();
    } else {
        // Try different macaroon locations as fallbacks
        let possible_macaroons = [
            testnet_dir.join("admin.macaroon"),
            testnet_dir.join("readonly.macaroon"),
            testnet_dir.join("data/chain/bitcoin/testnet/admin.macaroon"),
        ];
        
        for path in possible_macaroons.iter() {
            if path.exists() {
                connection.macaroon_path = path.to_string_lossy().to_string();
                break;
            }
        }
    }
    
    println!("Using cert path: {}", connection.cert_path);
    println!("Using macaroon path: {}", connection.macaroon_path);
    
    Ok(connection)
} 