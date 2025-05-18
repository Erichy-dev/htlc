use anyhow::{anyhow, Result};
use std::process::Command;
use std::process::Stdio;

pub struct NodeInfo {
    pub running: bool,
    pub version: String,
    pub synced: bool,
    pub block_height: u64,
    pub network: String,
    pub identity_pubkey: String,
}

pub fn node_status() -> NodeInfo {
    // Run lncli command and log results before starting UI
    let output = Command::new("lncli")
        .args(["--network", "testnet", "getinfo"])
        .output();

    // Default values for when command fails
    let mut node_running = false;
    let mut node_version = String::from("unknown");
    let mut is_synced = false;
    let mut block_height = 0;
    let mut network = String::from("testnet");
    let mut identity_pubkey = String::from("unknown");

    match output {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // println!("lncli --network testnet getinfo result:\n{}", stdout);

                // Set node as running since command was successful
                node_running = true;

                // Simple parsing of relevant fields
                if let Some(i) = stdout.find("\"version\":") {
                    if let Some(j) = stdout[i..].find(",") {
                        node_version = (&stdout[i + 11..i + j - 1]).trim_matches('"').to_string();
                    }
                }

                if let Some(i) = stdout.find("\"block_height\":") {
                    if let Some(j) = stdout[i..].find(",") {
                        if let Ok(height) = stdout[i + 15..i + j].trim().parse::<u64>() {
                            block_height = height;
                        }
                    }
                }

                if let Some(i) = stdout.find("\"synced_to_chain\":") {
                    is_synced = stdout[i + 17..i + 25].trim().contains("true");
                }

                if let Some(i) = stdout.find("\"identity_pubkey\":") {
                    if let Some(j) = stdout[i + 18..i + 40].find(",") {
                        identity_pubkey = (&stdout[i + 18..i + j - 1]).trim_matches('"').to_string();
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("lncli command failed: {}", stderr);
            }
        }
        Err(e) => println!("Failed to execute lncli: {}", e),
    }

    NodeInfo {
        running: node_running,
        version: node_version,
        synced: is_synced,
        block_height,
        network,
        identity_pubkey,
    }
}
