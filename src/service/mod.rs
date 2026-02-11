use anyhow::Result;
#[cfg(not(windows))]
use anyhow::anyhow;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum ServiceAccount {
    Inherit,
    LocalSystem,
    CurrentUser { username: String, password: String },
}

#[derive(Debug, Clone)]
pub struct ServiceInstallOptions {
    pub name: String,
    pub binary_path: PathBuf,
    pub arguments: String,
    pub working_directory: PathBuf,
    pub log_directory: PathBuf,
    pub account: ServiceAccount,
    pub auto_install_nssm: bool,
    pub autostart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub exists: bool,
    pub state: Option<String>,
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub fn install_service(options: &ServiceInstallOptions) -> Result<()> {
    windows::install_service(options)
}

#[cfg(not(windows))]
pub fn install_service(_options: &ServiceInstallOptions) -> Result<()> {
    Err(anyhow!(
        "Service management is currently supported on Windows only."
    ))
}

#[cfg(windows)]
pub fn remove_service(name: &str) -> Result<()> {
    windows::remove_service(name)
}

#[cfg(not(windows))]
pub fn remove_service(_name: &str) -> Result<()> {
    Err(anyhow!(
        "Service management is currently supported on Windows only."
    ))
}

#[cfg(windows)]
pub fn start_service(name: &str) -> Result<()> {
    windows::start_service(name)
}

#[cfg(not(windows))]
pub fn start_service(_name: &str) -> Result<()> {
    Err(anyhow!(
        "Service management is currently supported on Windows only."
    ))
}

#[cfg(windows)]
pub fn stop_service(name: &str) -> Result<()> {
    windows::stop_service(name)
}

#[cfg(not(windows))]
pub fn stop_service(_name: &str) -> Result<()> {
    Err(anyhow!(
        "Service management is currently supported on Windows only."
    ))
}

#[cfg(windows)]
pub fn restart_service(name: &str) -> Result<()> {
    windows::restart_service(name)
}

#[cfg(not(windows))]
pub fn restart_service(_name: &str) -> Result<()> {
    Err(anyhow!(
        "Service management is currently supported on Windows only."
    ))
}

#[cfg(windows)]
pub fn status_service(name: &str) -> Result<ServiceStatus> {
    windows::status_service(name)
}

#[cfg(not(windows))]
pub fn status_service(_name: &str) -> Result<ServiceStatus> {
    Err(anyhow!(
        "Service management is currently supported on Windows only."
    ))
}
