use crate::MainWindow;
use anyhow::{anyhow, Result};
use rand::RngCore;
use sha2::{Digest, Sha256};
use slint::{Timer, TimerMode};
use std::process::Command;
use std::time::Duration;
use std::path::Path;
use serde_json::Value;

pub fn generate_preimage() -> (String, String) {
    let mut rng = rand::thread_rng();
    let mut preimage = vec![0u8; 32];
    rng.fill_bytes(&mut preimage);
    
    let preimage_hex = hex::encode(&preimage);
    let hash = Sha256::digest(&preimage);
    let hash_hex = hex::encode(hash);
    
    (preimage_hex, hash_hex)
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

pub fn extract_funding_txid_from_string(json_string: &str) -> Option<String> {
    if let Ok(parsed_json) = serde_json::from_str::<Value>(json_string) {
        if let Some(txid) = parsed_json.get("funding_txid").and_then(|v| v.as_str()) {
            return Some(txid.to_string());
        }
        // Add other potential keys if the output varies
        if let Some(channel_point) = parsed_json.get("channel_point").and_then(|v| v.as_str()) {
             // Channel point is often <txid>:<index>
            return Some(channel_point.split(':').next().unwrap_or("").to_string());
        }
    }
    None
} 