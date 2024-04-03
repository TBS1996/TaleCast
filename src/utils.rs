use anyhow::Result;
use std::io::Write as IOWrite;
use std::path::PathBuf;

#[allow(dead_code)]
pub fn log<S: AsRef<str>>(message: S) -> Result<()> {
    let log_file_path = default_download_path()?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)?;
    writeln!(file, "{}", message.as_ref())?;
    Ok(())
}

fn config_dir() -> Result<PathBuf> {
    let p = dirs::config_dir()
        .ok_or(anyhow::Error::msg("no config dir found"))?
        .join(crate::APPNAME);
    std::fs::create_dir_all(&p)?;
    Ok(p)
}

pub fn podcasts_toml() -> Result<PathBuf> {
    Ok(config_dir()?.join("podcasts.toml"))
}

pub fn config_toml() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn current_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

pub fn default_download_path() -> Result<PathBuf> {
    let p = dirs::home_dir()
        .ok_or(anyhow::Error::msg("unable to get home directory"))?
        .join(crate::APPNAME);
    std::fs::create_dir_all(&p)?;
    Ok(p)
}
