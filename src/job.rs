use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use croner::Cron;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use std::{fs};
use thiserror::Error;
use crate::time::now_sydney;

#[derive(Debug, Error)]
pub enum JobError {
    #[error("Failed to parse cron expression: {0}")]
    CronParseError(String),
    
    #[error("Failed to execute HTTP request: {0}")]
    HttpError(#[from] reqwest::Error),
    
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Failed to parse job file: {0}")]
    ParseError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPayload {
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub cron_expression: String,
    pub endpoint: String,
    pub method: String,
    pub payload: JobPayload,
    pub timeout_seconds: Option<u64>,
    pub enabled: bool,
    
    #[serde(skip)]
    pub schedule: Option<Cron>,
    #[serde(skip)]
    pub last_run: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub next_run: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub last_status: Option<JobStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatus {
    pub success: bool,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: u64,
    pub response_code: Option<u16>,
    pub error_message: Option<String>,
}

impl Job {
    pub fn new(
        id: &str,
        name: &str,
        cron_expression: &str,
        endpoint: &str,
        method: &str,
    ) -> Result<Self, JobError> {
        let schedule = Cron::new(cron_expression)
			.with_seconds_required()
			.parse()
			.map_err(|e| JobError::CronParseError(e.to_string()))?;

		let next_run = schedule
			.iter_from(now_sydney())
			.next()
			.map(|dt| dt.with_timezone(&Utc));
        
        Ok(Job {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            cron_expression: cron_expression.to_string(),
            endpoint: endpoint.to_string(),
            method: method.to_string(),
            payload: JobPayload {
                headers: std::collections::HashMap::new(),
                body: None,
            },
            timeout_seconds: Some(30),
            enabled: true,
            schedule: Some(schedule),
            last_run: None,
            next_run,
            last_status: None,
        })
    }
    
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, JobError> {
        let content = fs::read_to_string(path)
            .map_err(|e| JobError::IoError(e))?;
        
        let mut job: Job = toml::from_str(&content)
            .map_err(|e| JobError::ParseError(e.to_string()))?;
        
        // Initialize the schedule
        job.schedule = Some(
            Cron::new(&job.cron_expression)
				.with_seconds_required()
				.parse()
				.map_err(|e| JobError::CronParseError(e.to_string()))?
        );
        
        // Calculate next run time
        if let Some(schedule) = &job.schedule {
            job.next_run = schedule
			.iter_from(now_sydney())
			.next()
			.map(|dt| dt.with_timezone(&Utc));
		}
        
        Ok(job)
    }
    
    pub fn is_due(&self) -> bool {
        if !self.enabled {
            return false;
        }
        
        if let Some(next_run) = self.next_run {
            return Utc::now() >= next_run;
        }
        
        false
    }
    
    pub fn update_schedule(&mut self) {
        if let Some(schedule) = &self.schedule {
            self.next_run = schedule
				.iter_from(now_sydney())
				.next()
				.map(|dt| dt.with_timezone(&Utc));
		}
    }
    
    pub async fn execute(&mut self, client: &Client) -> Result<JobStatus, JobError> {
        let start_time = Utc::now();
        let timeout = self.timeout_seconds.unwrap_or(30);
        
        log::info!("Executing job {}: {}", self.id, self.name);
        
        let result = match self.method.to_uppercase().as_str() {
            "GET" => {
                self.execute_get(client, timeout).await
            },
            "POST" => {
                self.execute_post(client, timeout).await
            },
            "PUT" => {
                self.execute_put(client, timeout).await
            },
            "DELETE" => {
                self.execute_delete(client, timeout).await
            },
            _ => {
                return Ok(JobStatus {
                    success: false,
                    timestamp: Utc::now(),
                    duration_ms: 0,
                    response_code: None,
                    error_message: Some(format!("Unsupported HTTP method: {}", self.method)),
                });
            }
        };
        
        let end_time = Utc::now();
        let duration = end_time.signed_duration_since(start_time);
        let duration_ms = duration.num_milliseconds() as u64;
        
        let status = match result {
            Ok(resp) => {
                let status_code = resp.status().as_u16();
                let success = resp.status().is_success();
				let response_text = resp.text().await.unwrap_or_else(|_| "Failed to read response".to_string());
                
                if success {
                    log::info!(
                        "Job {} completed successfully in {}ms with status code {}",
                        self.id, duration_ms, status_code
                    );
                } else {
                    log::warn!(
                        "Job {} completed with error status code {} in {}ms - Response: {}",
                        self.id, status_code, duration_ms, response_text
                    );
                }
                
                JobStatus {
                    success,
                    timestamp: end_time,
                    duration_ms,
                    response_code: Some(status_code),
                    error_message: None,
                }
            },
            Err(e) => {
                log::error!("Job {} failed: {}", self.id, e);
                JobStatus {
                    success: false,
                    timestamp: end_time,
                    duration_ms,
                    response_code: None,
                    error_message: Some(e.to_string()),
                }
            }
        };
        
        self.last_run = Some(end_time);
        self.last_status = Some(status.clone());
        self.update_schedule();
        
        Ok(status)
    }
    
    async fn execute_get(&self, client: &Client, timeout: u64) -> Result<reqwest::Response, reqwest::Error> {
        let mut request = client.get(&self.endpoint)
            .timeout(Duration::from_secs(timeout));
        
        // Add headers
        for (key, value) in &self.payload.headers {
            request = request.header(key, value);
        }
        
        request.send().await
    }
    
    async fn execute_post(&self, client: &Client, timeout: u64) -> Result<reqwest::Response, reqwest::Error> {
        let mut request = client.post(&self.endpoint)
            .timeout(Duration::from_secs(timeout));
        
        // Add headers
        for (key, value) in &self.payload.headers {
            request = request.header(key, value);
        }
        
        // Add body if exists
        if let Some(body) = &self.payload.body {
            request = request.json(body);
        }
        
        request.send().await
    }
    
    async fn execute_put(&self, client: &Client, timeout: u64) -> Result<reqwest::Response, reqwest::Error> {
        let mut request = client.put(&self.endpoint)
            .timeout(Duration::from_secs(timeout));
        
        // Add headers
        for (key, value) in &self.payload.headers {
            request = request.header(key, value);
        }
        
        // Add body if exists
        if let Some(body) = &self.payload.body {
            request = request.json(body);
        }
        
        request.send().await
    }
    
    async fn execute_delete(&self, client: &Client, timeout: u64) -> Result<reqwest::Response, reqwest::Error> {
        let mut request = client.delete(&self.endpoint)
            .timeout(Duration::from_secs(timeout));
        
        // Add headers
        for (key, value) in &self.payload.headers {
            request = request.header(key, value);
        }
        
        // Add body if exists
        if let Some(body) = &self.payload.body {
            request = request.json(body);
        }
        
        request.send().await
    }
}

pub fn load_jobs_from_directory<P: AsRef<Path>>(dir: P) -> Result<Vec<Job>> {
    let dir_path = dir.as_ref();
    let mut jobs = Vec::new();
    
    if !dir_path.exists() {
        fs::create_dir_all(dir_path).context("Failed to create jobs directory")?;
        return Ok(jobs);
    }
    
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().map_or(false, |ext| ext == "toml") {
            match Job::from_file(&path) {
                Ok(job) => {
                    log::info!("Loaded job: {} ({})", job.name, job.id);
                    jobs.push(job);
                },
                Err(e) => {
                    log::error!("Failed to load job from {}: {}", path.display(), e);
                }
            }
        }
    }
    
    Ok(jobs)
}