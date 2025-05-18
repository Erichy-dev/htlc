use anyhow::{anyhow, Context, Result};
use reqwest::Certificate;
use serde_json::json;
use std::{env, fs, time::Duration};

pub async fn unlock_wallet_rpc(
    password: &str,
) -> Result<()> {
    let lnd_rest_address = "https://localhost:8080".to_string();
    let home_dir = env::var("HOME").unwrap();
    let cert_file_path = format!("{}/Library/Application Support/Lnd/tls.cert", home_dir);

    let cert_pem = fs::read(&cert_file_path)
        .with_context(|| format!("Failed to read LND TLS cert from {}", cert_file_path))?;
    let lnd_cert = Certificate::from_pem(&cert_pem)
        .with_context(|| format!("Failed to parse LND TLS cert from PEM in {}", cert_file_path))?;

    println!(
        "Attempting LND wallet unlock via REST: {}",
        lnd_rest_address
    );

    let request_body = json!({
        "wallet_password": base64::encode(password.as_bytes()),
    });

    let client = reqwest::Client::builder()
        .add_root_certificate(lnd_cert)
        .build()
        .context("Failed to build reqwest client")?;

    let unlock_url = format!("{}/v1/unlockwallet", lnd_rest_address);

    match client
        .post(&unlock_url)
        .header("Content-Type", "application/json")
        .body(request_body.to_string())
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let response_text = response.text().await.unwrap_or_else(|e| e.to_string());

            if status.is_success() {
                println!("Wallet unlocked via REST. Status: {}.", status);
            } else {
                eprintln!(
                    "Unlock REST API failed. Status: {}. Response: {}",
                    status,
                    response_text
                );
            }
        }
        Err(e) => {
            eprintln!("Error sending unlock request to {}: {:#?}", unlock_url, e);
        }
    }

    Ok(())
}