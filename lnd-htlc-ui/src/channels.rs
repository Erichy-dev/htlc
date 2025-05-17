use anyhow::{anyhow, Result};
use std::process::Command;
use serde_json::Value;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ActiveChannelInfo {
    #[serde(rename = "chan_id")]
    pub channel_id: String,
    pub remote_pubkey: String,
    pub capacity: String,
    pub local_balance: String,
    pub remote_balance: String,
    pub active: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PendingChannelData {
    pub remote_node_pub: String,
    #[serde(default)] // channel_point might not be in all pending types uniformly at top level
    pub channel_point: String, 
    pub capacity: String,
    pub local_balance: String,
    #[serde(default)]
    pub remote_balance: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PendingChannelInfo {
    pub remote_node_pub: String,
    pub channel_point: String, 
    pub capacity: String,
    pub local_balance: String,
    pub remote_balance: String,
    pub status: String, 
}

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

pub fn list_active_channels() -> Result<Vec<ActiveChannelInfo>> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "listchannels"])
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: Value = serde_json::from_str(&stdout)
            .map_err(|e| anyhow!("Failed to parse active channels JSON: {}\nOutput: {}", e, stdout))?;
        
        let channels_val = json["channels"].as_array()
            .ok_or_else(|| anyhow!("No 'channels' array in listchannels output"))?;
        
        let mut channels_info = Vec::new();
        for chan_val in channels_val {
            // Ensure all fields are strings as expected by ActiveChannelInfo
            // or handle potential type mismatches if lncli output varies
            let capacity_str = chan_val["capacity"].as_str().unwrap_or("0").to_string();
            let local_balance_str = chan_val["local_balance"].as_str().unwrap_or("0").to_string();
            let remote_balance_str = chan_val["remote_balance"].as_str().unwrap_or("0").to_string();

            let channel: ActiveChannelInfo = ActiveChannelInfo {
                channel_id: chan_val["chan_id"].as_str().unwrap_or_default().to_string(),
                remote_pubkey: chan_val["remote_pubkey"].as_str().unwrap_or_default().to_string(),
                capacity: capacity_str,
                local_balance: local_balance_str,
                remote_balance: remote_balance_str,
                active: chan_val["active"].as_bool().unwrap_or(false),
            };
            channels_info.push(channel);
        }
        println!("Active Channels Info: {:?}", channels_info);
        Ok(channels_info)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("Failed to list active channels: {}", stderr))
    }
}

pub fn list_pending_channels() -> Result<Vec<PendingChannelInfo>> {
    let output = Command::new("lncli")
        .args(["--network", "testnet", "pendingchannels"])
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let json: Value = serde_json::from_str(&stdout)
            .map_err(|e| anyhow!("Failed to parse pending channels JSON: {}\nOutput: {}", e, stdout))?;

        let mut pending_infos = Vec::new();

        let process_pending_category = |category_key: &str, status_str: &str, pending_infos_vec: &mut Vec<PendingChannelInfo>, json_data: &Value| {
            if let Some(channels_array) = json_data[category_key].as_array() {
                for chan_val in channels_array {
                    let channel_data_val = &chan_val["channel"]; // Most pending channels have a nested "channel" object
                    
                    // Extract fields, providing defaults if they might be missing or have varying types
                    let remote_pubkey = channel_data_val["remote_node_pub"].as_str().unwrap_or_default().to_string();
                    let channel_point = channel_data_val["channel_point"].as_str().unwrap_or_default().to_string();
                    let capacity = channel_data_val["capacity"].as_str().unwrap_or("0").to_string();
                    let local_balance = channel_data_val["local_balance"].as_str().unwrap_or("0").to_string();
                    let remote_balance = channel_data_val["remote_balance"].as_str().unwrap_or("0").to_string(); // Often 0 or not present for pending

                    pending_infos_vec.push(PendingChannelInfo {
                        remote_node_pub: remote_pubkey,
                        channel_point: channel_point,
                        capacity: capacity,
                        local_balance: local_balance,
                        remote_balance: remote_balance,
                        status: status_str.to_string(),
                    });
                }
            }
        };

        process_pending_category("pending_open_channels", "Opening", &mut pending_infos, &json);
        process_pending_category("pending_closing_channels", "Closing", &mut pending_infos, &json);
        process_pending_category("pending_force_closing_channels", "Force Closing", &mut pending_infos, &json);
        process_pending_category("waiting_close_channels", "Waiting Close", &mut pending_infos, &json);

        println!("Pending Channels Info: {:?}", pending_infos);

        Ok(pending_infos)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!("Failed to list pending channels: {}", stderr))
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
