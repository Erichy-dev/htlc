use crate::MainWindow;
use anyhow::{anyhow, Result};
use rand::RngCore;
use sha2::{Digest, Sha256};
use slint::{Timer, TimerMode};
use std::process::Command;
use std::time::Duration;

pub fn generate_preimage() -> (String, String) {
    let mut rng = rand::thread_rng();
    let mut preimage = vec![0u8; 32];
    rng.fill_bytes(&mut preimage);
    
    let preimage_hex = hex::encode(&preimage);
    let hash = Sha256::digest(&preimage);
    let hash_hex = hex::encode(hash);
    
    (preimage_hex, hash_hex)
}

pub fn run_lncli(args: &[&str]) -> Result<String> {
    // Build command with custom RPC server settings
    let mut command = Command::new("lncli");
    
    // Add network flag
    command.arg("--network=testnet");
    
    // Add custom RPC server flag - adjust this based on your litd configuration
    // Use this if your LND RPC server is running on a non-default port
    command.arg("--rpcserver=127.0.0.1:10009");
    
    // Add the rest of the arguments
    command.args(args);
    
    let output = command.output()?;

    if !output.status.success() {
        return Err(anyhow!(
            "lncli command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8(output.stdout)?)
}

pub fn spawn_ui_timer<F>(window: &MainWindow, interval: Duration, callback: F)
where
    F: Fn() + 'static,
{
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, interval, move || {
        callback();
    });
} 