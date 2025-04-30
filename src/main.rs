mod config;
mod job;
mod logger;
mod monitor;
mod scheduler;
mod time;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the scheduler daemon
    Run,
    
    /// Show the monitor UI
    Monitor,
    
    /// List all jobs
    List,
    
    /// Create a new job template
    Create {
        /// ID of the job
        #[arg(short, long)]
        id: String,
        
        /// Name of the job
        #[arg(short, long)]
        name: String,
        
        /// Cron expression
        #[arg(short, long)]
        cron: String,
        
        /// HTTP endpoint
        #[arg(short, long)]
        endpoint: String,
        
        /// HTTP method
        #[arg(short, long, default_value = "GET")]
        method: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Load configuration
    let config = config::Config::load(&cli.config)?;
    
    // Initialize logger
    let app_log_path = format!("{}/app.log", config.logs_directory);
    let job_logs_dir = format!("{}/jobs", config.logs_directory);
    
    let _logger = logger::JobLogger::setup(
        app_log_path,
        job_logs_dir,
        config.get_log_level(),
    )?;
    
    // Create scheduler
    let scheduler = Arc::new(scheduler::Scheduler::new(
        &config.jobs_directory,
        config.check_interval_ms,
    ));
    
    match cli.command.unwrap_or(Commands::Run) {
        Commands::Run => {
            log::info!("Starting personal-cron scheduler");
            
            // Start file watcher for job configurations
            scheduler.start_file_watcher();
            
            // If monitor is enabled, start it in a separate task
            if config.monitor_enabled {
                let monitor_scheduler = Arc::clone(&scheduler);
                let update_interval = config.monitor_update_interval_ms;
                
                task::spawn(async move {
                    let mut monitor = monitor::Monitor::new(monitor_scheduler);
                    if let Err(e) = monitor.start(update_interval).await {
                        log::error!("Monitor error: {}", e);
                    }
                });
            }
            
            // Run the scheduler
            scheduler.start().await?;
        },
        Commands::Monitor => {
            // Load jobs initially
            scheduler.reload_jobs()?;
            
            // Start the monitor UI
            let mut monitor = monitor::Monitor::new(Arc::clone(&scheduler));
            monitor.start(config.monitor_update_interval_ms).await?;
        },
        Commands::List => {
            scheduler.reload_jobs()?;
            let jobs = scheduler.get_jobs();
            
			log::info!("Found {} jobs:", jobs.len());
            for job in jobs {
                let status = if let Some(status) = &job.last_status {
                    if status.success {
                        "Success".to_string()
                    } else {
                        format!("Failed: {}", status.error_message.as_deref().unwrap_or("Unknown"))
                    }
                } else {
                    "Never run".to_string()
                };
                log::info!(
					"{:<20} | {:<30} | {:<15} | {:<20}",
					job.id, job.name, job.cron_expression, status
				);
            }
        },
        Commands::Create { id, name, cron, endpoint, method } => {
            let job = job::Job::new(&id, &name, &cron, &endpoint, &method)?;
            
            // Convert to TOML
            let toml = toml::to_string_pretty(&job)?;
            
            // Create jobs directory if it doesn't exist
            std::fs::create_dir_all(&config.jobs_directory)?;
            
            // Write to file
            let path = format!("{}/{}.toml", config.jobs_directory, id);
            std::fs::write(&path, toml)?;
            
			log::info!("Created job file: {}", path);
        },
    }
    
    Ok(())
}