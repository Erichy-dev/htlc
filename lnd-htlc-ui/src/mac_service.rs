use std::{env, path::PathBuf, process::Command};

use anyhow::{Context, Result};

fn get_launch_agent_plist_path(network: &str) -> Result<PathBuf> {
    let home_dir = env::var("HOME").context("Failed to get HOME directory from environment variables")?;
    let mut path = PathBuf::from(home_dir);
    path.push("Library");
    path.push("LaunchAgents");
    path.push(format!("com.btc-{}.litd.plist", network));
    Ok(path)
}

fn get_launch_agents_dir_path() -> Result<PathBuf> {
    let home_dir = env::var("HOME").context("Failed to get HOME directory from environment variables")?;
    let mut path = PathBuf::from(home_dir);
    path.push("Library");
    path.push("LaunchAgents");
    Ok(path)
}

fn get_resource_path(filename: &str) -> PathBuf {
    // Get the path to the executable
    let exe_path = env::current_exe().expect("Failed to get current exe path");
    // Go up to the .app/Contents/
    let contents_dir = exe_path.parent().and_then(|p| p.parent()).expect("Failed to get Contents dir");
    // Go to Contents/Resources/
    let resources_dir = contents_dir.join("Resources");
    let resources_path = resources_dir.join(filename);

    // Check if file exists in Resources directory, otherwise use current directory
    if resources_path.exists() {
        resources_path
    } else {
        PathBuf::from(".").join(filename)
    }
}

pub fn start_mac_service(network: &str) -> Result<()> {
    let service_name = format!("com.btc-{}.litd", network);
    // Check if the service is already running
    // Using sh -c to correctly interpret pipes
    let check_output = Command::new("sh")
        .arg("-c")
        .arg(&format!("launchctl list | grep {}", service_name))
        .output()
        .context("Failed to execute launchctl list command")?;

    // If grep finds the string, it exits with 0. If not, it exits with 1.
    // We also print stdout/stderr for debugging.
    println!("{}", String::from_utf8_lossy(&check_output.stdout));
    println!("{}", String::from_utf8_lossy(&check_output.stderr));

    if check_output.status.success() { // success() means grep found the service
        println!("Service '{}' appears to be already loaded/running.", service_name);
        return Ok(());
    } else {
        println!("Service '{}' not found by 'launchctl list | grep', proceeding with setup.", service_name);
    }

    let plist_path = get_launch_agent_plist_path(network)?;
    let launch_agents_dir = get_launch_agents_dir_path()?;

    let service_file_name = write_service(network)?;
    let cp_output = Command::new("cp")
        .arg(&service_file_name) // Use the path to the plist in your project
        .arg(&launch_agents_dir) // Copy to the directory
        .output()
        .context(format!("Failed to copy plist to {:?}", launch_agents_dir))?;
    println!("{}", String::from_utf8_lossy(&cp_output.stdout));
    println!("{}", String::from_utf8_lossy(&cp_output.stderr));
    if !cp_output.status.success() {
        return Err(anyhow::anyhow!("Failed to copy plist file: {}", String::from_utf8_lossy(&cp_output.stderr)));
    }

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
        // It's possible the service is already loaded but not running, or some other error.
        // launchctl load can return non-zero if already loaded. Consider this not a fatal error
        // if the goal is to ensure it's loaded. Or, add more specific error handling.
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

fn write_service(network: &str) -> Result<String>{
    let service_file_name = format!("com.btc-{}.litd.plist", network);
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
                <string>litd</string>
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

    match std::fs::write(&service_file_name, content) {
        Ok(_) => Ok(service_file_name),
        Err(e) => Err(anyhow::anyhow!("Failed to write service file: {}", e)),
    }
}