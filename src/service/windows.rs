use super::{ServiceAccount, ServiceInstallOptions, ServiceStatus};
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::Duration;

fn output_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn run_command(program: &str, args: &[&str]) -> Result<Output> {
    Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute command: {program} {}", args.join(" ")))
}

fn run_checked(program: &str, args: &[&str]) -> Result<Output> {
    let output = run_command(program, args)?;
    if output.status.success() {
        return Ok(output);
    }

    Err(anyhow!(
        "command failed: {program} {}\nstdout: {}\nstderr: {}",
        args.join(" "),
        output_text(&output.stdout),
        output_text(&output.stderr),
    ))
}

fn is_marked_for_delete_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("marked for deletion") || message.contains("已标记为删除")
}

fn with_service_hints(error: anyhow::Error, service_name: &str) -> anyhow::Error {
    let message = error.to_string();
    if is_marked_for_delete_error(&message) {
        return anyhow!(
            "{message}\n\
             hint: service '{service_name}' is pending deletion.\n\
             close Services.msc / Event Viewer windows, wait a few seconds, then retry.\n\
             if it still persists, reboot Windows or use a different name via --name."
        );
    }

    let lower = message.to_lowercase();
    let is_access_denied = lower.contains("access is denied") || message.contains("拒绝访问");
    if is_access_denied {
        return anyhow!(
            "{message}\n\
             hint: run the terminal as Administrator and retry.\n\
             suggested: nanobot service install --name {service_name}"
        );
    }
    anyhow!(message)
}

fn command_exists(name: &str) -> bool {
    run_command("where", &[name])
        .map(|out| out.status.success() && !out.stdout.is_empty())
        .unwrap_or(false)
}

fn is_elevated() -> bool {
    run_command("net", &["session"])
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn ensure_admin(command_name: &str) -> Result<()> {
    if is_elevated() {
        return Ok(());
    }
    Err(anyhow!(
        "service {command_name} requires Administrator privileges.\n\
         hint: open PowerShell as Administrator and retry."
    ))
}

fn ensure_nssm(auto_install: bool) -> Result<()> {
    if command_exists("nssm") {
        return Ok(());
    }

    if !auto_install {
        return Err(anyhow!(
            "nssm is not installed. Install it with: winget install --id NSSM.NSSM -e"
        ));
    }

    if !command_exists("winget") {
        return Err(anyhow!(
            "nssm is missing and winget is unavailable. Install nssm manually first."
        ));
    }

    println!("nssm not found, installing via winget...");
    run_checked(
        "winget",
        &[
            "install",
            "--id",
            "NSSM.NSSM",
            "-e",
            "--accept-source-agreements",
            "--accept-package-agreements",
        ],
    )?;

    if !command_exists("nssm") {
        return Err(anyhow!(
            "nssm install command completed but nssm is still not found in PATH."
        ));
    }

    println!("nssm installed successfully.");
    Ok(())
}

fn service_exists(name: &str) -> bool {
    run_command("sc", &["query", name])
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn wait_for_service(name: &str, retries: usize, delay: Duration) -> bool {
    for _ in 0..retries {
        if service_exists(name) {
            return true;
        }
        sleep(delay);
    }
    false
}

fn parse_state(sc_output: &str) -> Option<String> {
    for line in sc_output.lines() {
        if !line.contains("STATE") {
            continue;
        }
        let (_, right) = line.split_once(':')?;
        let tokens = right.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            return None;
        }
        if tokens[0].chars().all(|c| c.is_ascii_digit()) {
            return tokens.get(1).map(|s| s.to_string());
        }
        return Some(tokens[0].to_string());
    }
    None
}

fn set_service_value(name: &str, key: &str, value: &str) -> Result<()> {
    run_checked("nssm", &["set", name, key, value]).map_err(|e| with_service_hints(e, name))?;
    Ok(())
}

fn set_service_values(name: &str, key: &str, values: &[&str]) -> Result<()> {
    let mut args = vec!["set", name, key];
    args.extend(values);
    run_checked("nssm", &args).map_err(|e| with_service_hints(e, name))?;
    Ok(())
}

fn set_service_account(name: &str, account: &ServiceAccount) -> Result<()> {
    match account {
        ServiceAccount::Inherit => Ok(()),
        ServiceAccount::LocalSystem => {
            run_checked("nssm", &["set", name, "ObjectName", "LocalSystem"])
                .map_err(|e| with_service_hints(e, name))?;
            Ok(())
        }
        ServiceAccount::CurrentUser { username, password } => {
            run_checked("nssm", &["set", name, "ObjectName", username, password])
                .map_err(|e| with_service_hints(e, name))?;
            Ok(())
        }
    }
}

pub fn install_service(options: &ServiceInstallOptions) -> Result<()> {
    ensure_admin("install")?;
    ensure_nssm(options.auto_install_nssm)?;
    fs::create_dir_all(&options.log_directory).with_context(|| {
        format!(
            "failed to create log directory: {}",
            options.log_directory.display()
        )
    })?;

    let binary = options.binary_path.to_string_lossy().to_string();
    let workdir = options.working_directory.to_string_lossy().to_string();
    let stdout_log = options
        .log_directory
        .join(format!("{}.out.log", options.name))
        .to_string_lossy()
        .to_string();
    let stderr_log = options
        .log_directory
        .join(format!("{}.err.log", options.name))
        .to_string_lossy()
        .to_string();

    if service_exists(&options.name) {
        println!(
            "Service '{}' already exists, updating configuration...",
            options.name
        );
    } else {
        let mut cmd = Command::new("nssm");
        cmd.arg("install").arg(&options.name).arg(&binary);
        if !options.arguments.trim().is_empty() {
            cmd.arg(options.arguments.trim());
        }
        let out = cmd.output().with_context(|| {
            format!(
                "failed to execute command: nssm install {} {}",
                options.name, binary
            )
        })?;
        if !out.status.success() {
            return Err(with_service_hints(
                anyhow!(
                    "command failed: nssm install {}\nstdout: {}\nstderr: {}",
                    options.name,
                    output_text(&out.stdout),
                    output_text(&out.stderr),
                ),
                &options.name,
            ));
        }
        if !wait_for_service(&options.name, 25, Duration::from_millis(200)) {
            return Err(anyhow!(
                "nssm install reported success but service '{}' is still unavailable.\n\
                 install stdout: {}\n\
                 install stderr: {}\n\
                 hint: run terminal as Administrator and retry.",
                options.name,
                output_text(&out.stdout),
                output_text(&out.stderr),
            ));
        }
        println!("Service '{}' installed.", options.name);
    }

    if !wait_for_service(&options.name, 10, Duration::from_millis(150)) {
        return Err(anyhow!(
            "service '{}' is not visible to SCM yet; cannot apply configuration",
            options.name
        ));
    }

    set_service_value(&options.name, "Application", &binary)?;
    if !options.arguments.trim().is_empty() {
        set_service_value(&options.name, "AppParameters", options.arguments.trim())?;
    }
    set_service_value(&options.name, "AppDirectory", &workdir)?;
    set_service_value(&options.name, "AppStdout", &stdout_log)?;
    set_service_value(&options.name, "AppStderr", &stderr_log)?;
    set_service_value(&options.name, "AppRotateFiles", "1")?;
    set_service_value(&options.name, "AppRotateOnline", "1")?;
    set_service_values(&options.name, "AppExit", &["Default", "Restart"])?;
    set_service_account(&options.name, &options.account)?;
    set_service_value(
        &options.name,
        "Start",
        if options.autostart {
            "SERVICE_AUTO_START"
        } else {
            "SERVICE_DEMAND_START"
        },
    )?;

    Ok(())
}

pub fn remove_service(name: &str) -> Result<()> {
    ensure_admin("remove")?;
    ensure_nssm(false)?;
    if !service_exists(name) {
        println!("Service '{}' is not installed.", name);
        return Ok(());
    }
    if let Err(err) = run_checked("nssm", &["remove", name, "confirm"]) {
        let msg = err.to_string();
        if is_marked_for_delete_error(&msg) {
            println!(
                "Service '{}' is already marked for deletion. It will disappear shortly.",
                name
            );
            return Ok(());
        }
        return Err(with_service_hints(err, name));
    }
    Ok(())
}

pub fn start_service(name: &str) -> Result<()> {
    ensure_admin("start")?;
    ensure_nssm(false)?;
    if !service_exists(name) {
        return Err(anyhow!("service '{}' is not installed", name));
    }
    run_checked("nssm", &["start", name])?;
    Ok(())
}

pub fn stop_service(name: &str) -> Result<()> {
    ensure_admin("stop")?;
    ensure_nssm(false)?;
    if !service_exists(name) {
        return Err(anyhow!("service '{}' is not installed", name));
    }
    run_checked("nssm", &["stop", name])?;
    Ok(())
}

pub fn restart_service(name: &str) -> Result<()> {
    ensure_admin("restart")?;
    ensure_nssm(false)?;
    if !service_exists(name) {
        return Err(anyhow!("service '{}' is not installed", name));
    }
    let current = status_service(name)?;
    if current.state.as_deref() == Some("RUNNING") {
        run_checked("nssm", &["stop", name])?;
    }
    run_checked("nssm", &["start", name])?;
    Ok(())
}

pub fn status_service(name: &str) -> Result<ServiceStatus> {
    if !service_exists(name) {
        return Ok(ServiceStatus {
            exists: false,
            state: None,
        });
    }
    let output = run_checked("sc", &["query", name])?;
    let state = parse_state(&String::from_utf8_lossy(&output.stdout));
    Ok(ServiceStatus {
        exists: true,
        state,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_state;

    #[test]
    fn parse_state_extracts_running() {
        let sample = "SERVICE_NAME: nanobot-rs\n        STATE              : 4  RUNNING";
        assert_eq!(parse_state(sample).as_deref(), Some("RUNNING"));
    }

    #[test]
    fn parse_state_handles_missing_line() {
        let sample = "SERVICE_NAME: nanobot-rs";
        assert_eq!(parse_state(sample), None);
    }
}
