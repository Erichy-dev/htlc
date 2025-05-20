use std::{path::PathBuf, process::Command};
use anyhow::{Result, Context};

use crate::{mac_service::start_mac_service, windows_service::start_windows_service};

pub fn start_litd_service(network: &str) -> Result<()> {
    // Determine the os type
    let os_type = Command::new("uname")
        .arg("-s")
        .output()
        .context("Failed to execute uname command")?;
    let os_type = String::from_utf8_lossy(&os_type.stdout).trim().to_string();
    println!("OS type: {}", os_type);

    if os_type == "Darwin" {
        start_mac_service(network)?;
    } else if os_type == "Windows" {
        start_windows_service(network)?;
    } else {
        return Err(anyhow::anyhow!("This script is only supported on macOS and Windows"));
    }

    Ok(())
}

pub fn stop_litd_service() -> Result<()> {
    let unload_output = Command::new("launchctl")
        .arg("remove")
        .arg("com.btc.litd")
        .output()
        .context("Failed to remove service with launchctl")?;
    println!("{}", String::from_utf8_lossy(&unload_output.stdout));
    println!("{}", String::from_utf8_lossy(&unload_output.stderr));
    if !unload_output.status.success() {
        // It's possible it was already unloaded.
         println!("Warning: 'launchctl unload' exited with non-zero status. Stderr: {}", String::from_utf8_lossy(&unload_output.stderr));
    }

    Ok(())
}

pub async fn get_network(db: &sled::Db) -> Result<String> {
    let network = db.get(b"network")?.unwrap_or(sled::IVec::from(b"testnet"));
    let network_str = String::from_utf8(network.to_vec()).unwrap_or_else(|_| "testnet".to_string());
    Ok(network_str)
}

pub async fn send_custom_message(message: String, identity_pubkey: String) -> Result<()> {
    // uses lncli --network testnet sendcustom --peer 038d276e996b87761a9c0c742a75f58f5ae39e0574448c7eae34c63ed1adb5753d --data $(echo -n "hello there" | xxd -p)
    // Convert message to hex using echo and xxd
    let hex_output = Command::new("sh")
        .arg("-c")
        .arg(format!("echo -n \"{}\" | xxd -p", message))
        .output()
        .context("Failed to convert message to hex")?;
    let hex_message = String::from_utf8_lossy(&hex_output.stdout).trim().to_string();

    let output = Command::new("lncli")
        .arg("--network")
        .arg("testnet")
        .arg("sendcustom") 
        .arg("--peer")
        .arg(identity_pubkey)
        .arg("--data")
        .arg(hex_message)
        .arg("--type")
        .arg("32768")
        .output()
        .context("Failed to send custom message")?;
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}