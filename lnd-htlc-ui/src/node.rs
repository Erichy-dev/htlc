use anyhow::{anyhow, Result};
use std::env;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;

use crate::unlock_wallet::unlock_wallet_rpc;
use crate::MainWindow;

#[derive(Clone)]
pub struct NodeInfo {
    pub running: bool,
    pub version: String,
    pub synced: bool,
    pub block_height: u64,
    pub network: String,
    pub identity_pubkey: String,
}

pub async fn node_status(network: &str, window_weak: &Arc<slint::Weak<MainWindow>>) -> NodeInfo {
    // Run lncli command and log results before starting UI
    let output = if network == "testnet" {
        Command::new("/usr/local/bin/lncli")
            .args(["--network", "testnet", "getinfo"])
            .output()
    } else {
        Command::new("/usr/local/bin/lncli")
            .args(["getinfo"])
            .output()
    };

    // Default values for when command fails
    let mut node_running = false;
    let mut node_version = String::from("unknown");
    let mut is_synced = false;
    let mut block_height = 0;
    let mut network = String::from("testnet");
    let mut identity_pubkey = String::from("unknown");

    let window_weak_clone = window_weak.clone();

    match output {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // println!("lncli --network testnet getinfo result:\n{}", stdout);

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
                    identity_pubkey = stdout[i + 21..i + 87].to_string();
                }
                let node_window_weak = window_weak_clone.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = node_window_weak.upgrade() {
                        window.set_wallet_needs_unlock(false);
                    }
                });

                let wallet_output = if network == "testnet" {
                    Command::new("/usr/local/bin/lncli")
                        .args(["--network", "testnet", "walletbalance"])
                        .output()
                } else {
                    Command::new("/usr/local/bin/lncli")
                        .args(["walletbalance"])
                        .output()
                };

                let mut wallet_balance: i32 = 0;
                match wallet_output {
                    Ok(wallet_output) => {
                        if wallet_output.status.success() {
                            let stdout = String::from_utf8_lossy(&wallet_output.stdout);
                            const CONFIRMED_BALANCE_KEY: &str = "\"confirmed_balance\":  \"";
                            if let Some(key_start_index) = stdout.find(CONFIRMED_BALANCE_KEY) {
                                let value_start_index = key_start_index + CONFIRMED_BALANCE_KEY.len();

                                let possible_balance = stdout[value_start_index..].trim();
                                let index_of_quote = possible_balance.find("\"");
                                let balance_str = &possible_balance[..index_of_quote.unwrap()].trim();

                                if let Ok(balance) = balance_str.parse::<i32>() {
                                    wallet_balance = balance;
                                } else {
                                    println!("Failed to parse confirmed balance from string: '{}'", balance_str);
                                }
                            } else {
                                println!("'confirmed_balance' key not found in walletbalance output.");
                            }
                        }
                    }
                    Err(e) => {
                        println!("Failed to get wallet balance: {}", e);
                    }
                }
                let wallet_window_weak = window_weak_clone.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = wallet_window_weak.upgrade() {
                        window.set_wallet_balance(wallet_balance);
                    }
                });
                
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("lncli command failed: {}", stderr);
                if stderr.contains("unlock it") {
                    println!("Wallet is locked");
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = window_weak_clone.upgrade() {
                            window.set_wallet_needs_unlock(true);
                        }
                    });
                }
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
