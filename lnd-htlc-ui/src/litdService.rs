use std::process::Command;
use std::path::PathBuf;
use std::env;
use anyhow::{Result, Context};

fn get_launch_agent_plist_path() -> Result<PathBuf> {
    let home_dir = env::var("HOME").context("Failed to get HOME directory from environment variables")?;
    let mut path = PathBuf::from(home_dir);
    path.push("Library");
    path.push("LaunchAgents");
    path.push("com.btc.litd.plist");
    Ok(path)
}

fn get_launch_agents_dir_path() -> Result<PathBuf> {
    let home_dir = env::var("HOME").context("Failed to get HOME directory from environment variables")?;
    let mut path = PathBuf::from(home_dir);
    path.push("Library");
    path.push("LaunchAgents");
    Ok(path)
}

pub fn start_litd_service() -> Result<()> {
    // Determine the os type
    let os_type = Command::new("uname")
        .arg("-s")
        .output()
        .context("Failed to execute uname command")?;
    let os_type = String::from_utf8_lossy(&os_type.stdout).trim().to_string();
    println!("OS type: {}", os_type);

    if os_type != "Darwin" {
        return Err(anyhow::anyhow!("This script is only supported on macOS"));
    }

    // Check if the service is already running
    // Using sh -c to correctly interpret pipes
    let check_output = Command::new("sh")
        .arg("-c")
        .arg("launchctl list | grep com.btc.litd")
        .output()
        .context("Failed to execute launchctl list command")?;

    // If grep finds the string, it exits with 0. If not, it exits with 1.
    // We also print stdout/stderr for debugging.
    println!("{}", String::from_utf8_lossy(&check_output.stdout));
    println!("{}", String::from_utf8_lossy(&check_output.stderr));

    if check_output.status.success() { // success() means grep found the service
        println!("Service 'com.btc.litd' appears to be already loaded/running.");
        return Ok(());
    } else {
        println!("Service 'com.btc.litd' not found by 'launchctl list | grep', proceeding with setup.");
    }

    let plist_path = get_launch_agent_plist_path()?;
    let launch_agents_dir = get_launch_agents_dir_path()?;

    // Ensure the source plist file exists in the current directory or expected location
    let source_plist_path = PathBuf::from("com.btc.litd.plist");
    if !source_plist_path.exists() {
        return Err(anyhow::anyhow!("Source plist file 'com.btc.litd.plist' not found in current directory."));
    }

    let cp_output = Command::new("cp")
        .arg(&source_plist_path) // Use the path to the plist in your project
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
        .arg("com.btc.litd") // Use the label here
        .output()
        .context("Failed to start service with launchctl start")?;
    println!("{}", String::from_utf8_lossy(&start_output.stdout));
    println!("{}", String::from_utf8_lossy(&start_output.stderr));
    if !start_output.status.success() {
        println!("Warning: 'launchctl start' exited with non-zero status. Stderr: {}", String::from_utf8_lossy(&start_output.stderr));
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