use anyhow::Result;
use chrono::{DateTime, Utc};
use log::{LevelFilter, Record};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct JobLogger {
    app_log_path: String,
    job_logs_dir: String,
    log_file: Arc<Mutex<File>>,
}

impl Clone for JobLogger {
    fn clone(&self) -> Self {
        JobLogger {
            app_log_path: self.app_log_path.clone(),
            job_logs_dir: self.job_logs_dir.clone(),
            log_file: Arc::clone(&self.log_file),
        }
    }
}

pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub message: String,
    pub job_id: Option<String>,
}

impl JobLogger {
    pub fn setup<P1: AsRef<Path>, P2: AsRef<Path>>(
        app_log_path: P1,
        job_logs_dir: P2,
        level: LevelFilter,
    ) -> Result<Self> {
        // Ensure directories exist
        if let Some(parent) = Path::new(app_log_path.as_ref()).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(job_logs_dir.as_ref())?;
        
        // Setup app log file
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(app_log_path.as_ref())?;
        
		// Clone the logger
		let logger_base = JobLogger {
			app_log_path: app_log_path.as_ref().to_string_lossy().to_string(),
			job_logs_dir: job_logs_dir.as_ref().to_string_lossy().to_string(),
			log_file: Arc::new(Mutex::new(log_file)),
		};

		// Create a clone to use in the closure
		let logger = logger_base.clone();
        
        // Setup global logger
        env_logger::Builder::new()
            .filter(None, level)
            .format(move |buf, record| {
                let _ = logger.log_record(record);
                let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");

				Ok(())
            })
            .init();
        
        log::info!("Logger initialized with app log: {}", logger_base.app_log_path);
        log::info!("Job logs directory: {}", logger_base.job_logs_dir);
        
        Ok(logger_base)
    }
    
    fn log_record(&self, record: &Record) -> Result<()> {
        let timestamp = Utc::now();
        let message = format!("{}", record.args());
        
        // Check if this is a job-specific log
        let job_id = if message.starts_with("Job ") && message.contains(':') {
            let parts: Vec<&str> = message.splitn(3, ' ').collect();
            if parts.len() >= 2 {
                let id_part = parts[1].trim_end_matches(':');
                Some(id_part.to_string())
            } else {
                None
            }
        } else {
            None
        };
        
        // Write to main log file
        let mut file = self.log_file.lock().unwrap();
        writeln!(
            file,
            "[{} {}] {}",
            timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            message
        )?;
        
        // If job-specific, also write to job log file
        if let Some(job_id) = job_id {
            self.log_job_message(&job_id, record.level().to_string(), &message, timestamp)?;
        }
        
        Ok(())
    }
    
    fn log_job_message(
        &self,
        job_id: &str,
        level: String,
        message: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        let job_log_path = Path::new(&self.job_logs_dir).join(format!("{}.log", job_id));
        
        let mut job_log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(job_log_path)?;
        
        writeln!(
            job_log_file,
            "[{} {}] {}",
            timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            level,
            message
        )?;
        
        Ok(())
    }
    
    pub fn get_recent_logs(&self, count: usize) -> Result<Vec<LogEntry>> {
        let log_content = fs::read_to_string(&self.app_log_path)?;
        let mut entries = Vec::new();
        
        for line in log_content.lines().rev().take(count) {
            if let Some(entry) = self.parse_log_line(line) {
                entries.push(entry);
            }
        }
        
        entries.reverse();
        Ok(entries)
    }
    
    pub fn get_job_logs(&self, job_id: &str, count: usize) -> Result<Vec<LogEntry>> {
        let job_log_path = Path::new(&self.job_logs_dir).join(format!("{}.log", job_id));
        
        if !job_log_path.exists() {
            return Ok(Vec::new());
        }
        
        let log_content = fs::read_to_string(job_log_path)?;
        let mut entries = Vec::new();
        
        for line in log_content.lines().rev().take(count) {
            if let Some(mut entry) = self.parse_log_line(line) {
                entry.job_id = Some(job_id.to_string());
                entries.push(entry);
            }
        }
        
        entries.reverse();
        Ok(entries)
    }
    
    fn parse_log_line(&self, line: &str) -> Option<LogEntry> {
        // Expected format: [YYYY-MM-DD HH:MM:SS.sss LEVEL] MESSAGE
        if !line.starts_with('[') || !line.contains(']') {
            return None;
        }
        
        let parts: Vec<&str> = line.splitn(2, ']').collect();
        if parts.len() != 2 {
            return None;
        }
        
        let header = parts[0].trim_start_matches('[');
        let message = parts[1].trim();
        
        let header_parts: Vec<&str> = header.rsplitn(2, ' ').collect();
        if header_parts.len() != 2 {
            return None;
        }
        
        let level = header_parts[0];
        let timestamp_str = header_parts[1];
        
        if let Ok(timestamp) = chrono::DateTime::parse_from_str(
            &format!("{} +0000", timestamp_str),
            "%Y-%m-%d %H:%M:%S%.3f %z"
        ) {
            return Some(LogEntry {
                timestamp: timestamp.with_timezone(&Utc),
                level: level.to_string(),
                message: message.to_string(),
                job_id: None,
            });
        }
        
        None
    }
}