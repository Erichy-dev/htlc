use anyhow::{anyhow, Result};
use std::process::Command;
use std::fs;
use std::io::Write;
use std::time::Duration;

pub fn check_litd_config() -> Result<bool> {
    // Get user's home directory
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not determine user's home directory"))?;

    // Check if the .lit directory exists, create if not
    let lit_dir = home.join(".lit");
    if !lit_dir.exists() {
        fs::create_dir_all(&lit_dir)?;
    }

    // Check if the lit.conf file exists
    let conf_path = lit_dir.join("lit.conf");
    if !conf_path.exists() {
        // Create a default config file
        let mut file = fs::File::create(&conf_path)?;
        let default_config = r#"httpslisten=0.0.0.0:8443
uipassword=password_change_me
lnd-mode=integrated
lnd.bitcoin.active=1
lnd.bitcoin.testnet=1
lnd.bitcoin.node=neutrino
lnd.feeurl=https://nodes.lightning.computer/fees/v1/btc-fee-estimates.json
lnd.protocol.option-scid-alias=true
lnd.protocol.zero-conf=true
"#;
        file.write_all(default_config.as_bytes())?;
        
        return Ok(false); // Indicates we created a new config
    }

    Ok(true) // Config already existed
}

pub fn is_wallet_locked() -> Result<bool> {
    // Method 1: Try to run getinfo - if it fails with a wallet locked error, return true
    let getinfo_output = Command::new("lncli")
        .arg("--network=testnet")
        .arg("--rpcserver=127.0.0.1:10009")
        .arg("getinfo")
        .output()?;
    
    // Check for wallet locked error in stderr
    let stderr = String::from_utf8_lossy(&getinfo_output.stderr);
    
    // If the wallet is locked, lncli will return an error with "wallet locked" in it
    if stderr.contains("wallet locked") || 
       stderr.contains("wallet not unlocked") ||
       stderr.contains("wallet state: LOCKED") ||
       stderr.contains("unlock it to enable full RPC access") {
        println!("Wallet is locked based on getinfo error message");
        return Ok(true);
    }
    
    // Method 2: Try listchannels - wallet operations require unlocked wallet
    let channels_output = Command::new("lncli")
        .arg("--network=testnet")
        .arg("--rpcserver=127.0.0.1:10009")
        .arg("listchannels")
        .output()?;
    
    let channels_stderr = String::from_utf8_lossy(&channels_output.stderr);
    
    if channels_stderr.contains("wallet locked") || 
       channels_stderr.contains("wallet not unlocked") ||
       channels_stderr.contains("wallet state: LOCKED") ||
       channels_stderr.contains("unlock it to enable full RPC access") {
        println!("Wallet is locked based on listchannels error message");
        return Ok(true);
    }
    
    // If we can run commands successfully, wallet is likely unlocked
    Ok(false)
}

pub fn unlock_wallet(password: &str) -> Result<bool> {
    println!("Attempting to unlock wallet...");
    
    let output = Command::new("lncli")
        .arg("--network=testnet")
        .arg("--rpcserver=127.0.0.1:10009")
        .arg("unlock")
        .arg("--password")
        .arg(password)
        .output()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    println!("Unlock command stdout: {}", stdout);
    println!("Unlock command stderr: {}", stderr);
    
    // Check if unlock was successful
    if output.status.success() {
        println!("Wallet unlock successful");
        
        // Verify the wallet is actually unlocked by checking status
        std::thread::sleep(Duration::from_secs(1)); // Give it a moment to take effect
        return match is_wallet_locked() {
            Ok(still_locked) => {
                if still_locked {
                    println!("Wallet appears to still be locked after unlock command");
                    Ok(false)
                } else {
                    println!("Wallet lock state verified as unlocked");
                    Ok(true)
                }
            },
            Err(e) => {
                println!("Error verifying wallet state: {}", e);
                Ok(false)
            }
        };
    }
    
    // If there was an error, check if it's because the wallet is already unlocked
    if stderr.contains("wallet already unlocked") {
        println!("Wallet was already unlocked");
        return Ok(true); // It's already unlocked, which is what we want
    }
    
    // Otherwise, something went wrong
    println!("Wallet unlock failed");
    Ok(false)
} 