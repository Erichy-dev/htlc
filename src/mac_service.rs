use std::{env, path::PathBuf, process::Command};

use anyhow::{Context, Result};
use directories::{ProjectDirs, UserDirs};

use crate::get_app_data_dir;

pub fn start_mac_service(network: &str) -> Result<()> {
    let service_name = format!("com.btc-{}.litd", network);
    // Check if the service is already running
    // Using sh -c to correctly interpret pipes
    let check_output = Command::new("sh")
        .arg("-c")
        .arg(&format!("launchctl list | grep {}", service_name))
        .output()
        .context("Failed to execute launchctl list command")?;

    println!("{}", String::from_utf8_lossy(&check_output.stdout));
    println!("{}", String::from_utf8_lossy(&check_output.stderr));

    if check_output.status.success() { // success() means grep found the service
        println!("Service '{}' appears to be already loaded/running.", service_name);
        // return Ok(());
    } else {
        println!("Service '{}' not found by 'launchctl list | grep', proceeding with setup.", service_name);
    }

    let plist_path = write_service(network)?;

    let chmod_output = Command::new("chmod")
        .arg("644")
        .arg(&plist_path)
        .output()
        .context(format!("Failed to chmod plist at {:?}", plist_path))?;
    println!("{}", String::from_utf8_lossy(&chmod_output.stdout));
    println!("{}", String::from_utf8_lossy(&chmod_output.stderr));
    if !chmod_output.status.success() {
        return Err(anyhow::anyhow!("Failed to chmod plist file: {}", String::from_utf8_lossy(&chmod_output.stderr)));
    }

    let load_output = Command::new("launchctl")
        .arg("load")
        .arg(&plist_path)
        .output()
        .context(format!("Failed to load plist with launchctl from {:?}", plist_path))?;
    println!("{}", String::from_utf8_lossy(&load_output.stdout));
    println!("{}", String::from_utf8_lossy(&load_output.stderr));
     if !load_output.status.success() {
        println!("Warning: 'launchctl load' exited with non-zero status. This might be okay if already loaded. Stderr: {}", String::from_utf8_lossy(&load_output.stderr));
    }

    // Attempt to start the service explicitly
    let start_output = Command::new("launchctl")
        .arg("start")
        .arg(&service_name) // Use the label here
        .output()
        .context("Failed to start service with launchctl start")?;
    println!("{}", String::from_utf8_lossy(&start_output.stdout));
    println!("{}", String::from_utf8_lossy(&start_output.stderr));
    if !start_output.status.success() {
        println!("Warning: 'launchctl start' exited with non-zero status. Stderr: {}", String::from_utf8_lossy(&start_output.stderr));
    }

    Ok(())
}

fn write_service(network: &str) -> Result<PathBuf>{
    let launch_agents_dir = UserDirs::new().unwrap().home_dir().join("Library").join("LaunchAgents");
    let service_file_name = format!("com.btc-{}.litd.plist", network);
    let plist_path = launch_agents_dir.join(service_file_name);
    let content = if network == "mainnet" { 
        r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0">
        <dict>
            <key>Label</key>
            <string>com.btc.litd</string>
            <key>ProgramArguments</key>
            <array>
                <string>/usr/local/bin/litd</string>
            </array>
            <key>RunAtLoad</key>
            <true/>
            <key>KeepAlive</key>
            <true/>
            <key>StandardOutPath</key>
            <string>/tmp/com.btc.litd.stdout.log</string>
            <key>StandardErrorPath</key>
            <string>/tmp/com.btc.litd.stderr.log</string>
        </dict>
        </plist>
        "#
    } else {
        r#"
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0">
        <dict>
            <key>Label</key>
            <string>com.btc.litd</string>
            <key>ProgramArguments</key>
            <array>
                <string>/usr/local/bin/litd</string>
                <string>--network</string>
                <string>testnet</string>
            </array>
            <key>RunAtLoad</key>
            <true/>
            <key>KeepAlive</key>
            <true/>
            <key>StandardOutPath</key>
            <string>/tmp/com.btc.litd.stdout.log</string>
            <key>StandardErrorPath</key>
            <string>/tmp/com.btc.litd.stderr.log</string>
        </dict>
        </plist>
        "#
    };

    match std::fs::write(&plist_path, content) {
        Ok(_) => Ok(plist_path),
        Err(e) => Err(anyhow::anyhow!("Failed to write service file: {}", e)),
    }
}