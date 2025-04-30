use crate::job::{Job, JobStatus};
use anyhow::Result;
use reqwest::Client;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time;

pub struct Scheduler {
    jobs: Arc<Mutex<Vec<Job>>>,
    http_client: Client,
    job_statuses: Arc<Mutex<HashMap<String, Vec<JobStatus>>>>,
    jobs_dir: String,
    check_interval_ms: u64,
}

impl Scheduler {
    pub fn new<P: AsRef<Path>>(jobs_dir: P, check_interval_ms: u64) -> Self {
        Scheduler {
            jobs: Arc::new(Mutex::new(Vec::new())),
            http_client: Client::new(),
            job_statuses: Arc::new(Mutex::new(HashMap::new())),
            jobs_dir: jobs_dir.as_ref().to_string_lossy().to_string(),
            check_interval_ms,
        }
    }

    pub fn reload_jobs(&self) -> Result<()> {
        let mut jobs = self.jobs.lock().unwrap();
        *jobs = crate::job::load_jobs_from_directory(&self.jobs_dir)?;
        log::info!("Loaded {} jobs from {}", jobs.len(), self.jobs_dir);
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        log::info!("Starting scheduler with check interval of {}ms", self.check_interval_ms);
        self.reload_jobs()?;
        
        let mut interval = time::interval(Duration::from_millis(self.check_interval_ms));
        
        loop {
            interval.tick().await;
            self.check_and_execute_jobs().await?;
        }
    }
    
    async fn check_and_execute_jobs(&self) -> Result<()> {
        let mut jobs_to_execute = Vec::new();
        
        // Find jobs that are due to run
        {
            let mut jobs = self.jobs.lock().unwrap();
            for job in jobs.iter_mut() {
                if job.is_due() {
                    jobs_to_execute.push(job.clone());
                }
            }
        }
        
        // Execute due jobs
        for mut job in jobs_to_execute {
            match job.execute(&self.http_client).await {
                Ok(status) => {
                    // Store the status
                    let mut statuses = self.job_statuses.lock().unwrap();
                    let job_history = statuses.entry(job.id.clone()).or_insert_with(Vec::new);
                    job_history.push(status);
                    
                    // Keep only the last 100 statuses
                    if job_history.len() > 100 {
                        job_history.remove(0);
                    }
                    
                    // Update the job in the jobs list with new next_run time
                    let mut jobs = self.jobs.lock().unwrap();
                    if let Some(existing_job) = jobs.iter_mut().find(|j| j.id == job.id) {
                        existing_job.last_run = job.last_run;
                        existing_job.next_run = job.next_run;
                        existing_job.last_status = job.last_status;
                    }
                },
                Err(e) => {
                    log::error!("Error executing job {}: {}", job.id, e);
                }
            }
        }
        
        Ok(())
    }
    
    pub fn get_jobs(&self) -> Vec<Job> {
        let jobs = self.jobs.lock().unwrap();
        jobs.clone()
    }
    
    pub fn get_job_status_history(&self, job_id: &str) -> Option<Vec<JobStatus>> {
        let statuses = self.job_statuses.lock().unwrap();
        statuses.get(job_id).cloned()
    }
    
    // Start file watcher to automatically reload jobs when files change
    pub fn start_file_watcher(&self) -> Result<()> {
        use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc::channel;
        use std::thread;
        
        let jobs_dir = self.jobs_dir.clone();
        let scheduler = Arc::new(self.clone());
        
        thread::spawn(move || -> Result<(), anyhow::Error> {
            let (tx, rx) = channel();
            
            let mut watcher = RecommendedWatcher::new(
                tx,
                Config::default(),
            )?;
            
            watcher.watch(Path::new(&jobs_dir), RecursiveMode::Recursive)?;
            
            log::info!("File watcher started for directory: {}", jobs_dir);
            
            loop {
                match rx.recv() {
                    Ok(_) => {
                        log::info!("Job file changes detected, reloading jobs");
                        if let Err(e) = scheduler.reload_jobs() {
                            log::error!("Failed to reload jobs: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Watch error: {:?}", e);
                        break;
                    }
                }
            }
            
            Ok(())
        });
        
        Ok(())
    }
}

impl Clone for Scheduler {
    fn clone(&self) -> Self {
        Scheduler {
            jobs: Arc::clone(&self.jobs),
            http_client: Client::new(),
            job_statuses: Arc::clone(&self.job_statuses),
            jobs_dir: self.jobs_dir.clone(),
            check_interval_ms: self.check_interval_ms,
        }
    }
}