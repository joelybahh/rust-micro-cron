use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub jobs_directory: String,
    pub logs_directory: String,
    pub check_interval_ms: u64,
    pub log_level: String,
    pub monitor_enabled: bool,
    pub monitor_update_interval_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            jobs_directory: "cron_jobs".to_string(),
            logs_directory: "logs".to_string(),
            check_interval_ms: 1000,
            log_level: "info".to_string(),
            monitor_enabled: true,
            monitor_update_interval_ms: 1000,
        }
    }
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        if !path.exists() {
            let config = Config::default();
            config.save(path)?;
            return Ok(config);
        }
        
        let content = fs::read_to_string(path).context("Failed to read config file")?;
        let config: Config = toml::from_str(&content).context("Failed to parse config file")?;
        
        Ok(config)
    }
    
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }
        
        fs::write(path, content).context("Failed to write config file")?;
        Ok(())
    }
    
    pub fn get_log_level(&self) -> log::LevelFilter {
        match self.log_level.to_lowercase().as_str() {
            "trace" => log::LevelFilter::Trace,
            "debug" => log::LevelFilter::Debug,
            "info" => log::LevelFilter::Info,
            "warn" => log::LevelFilter::Warn,
            "error" => log::LevelFilter::Error,
            _ => log::LevelFilter::Info,
        }
    }
}