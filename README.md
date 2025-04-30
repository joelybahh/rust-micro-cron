# Personal CRON Job Manager

A lightweight personal CRON job manager written in Rust. This application allows you to define and manage HTTP-based CRON jobs, track their execution status, and view detailed logs.

## Features

- CRON job scheduling with standard syntax
- HTTP endpoint execution (GET, POST, PUT, DELETE)
- Configurable payloads and headers
- Job execution logging
- Terminal-based UI for monitoring
- Automatic reloading of job configurations
- Error tracking and reporting

## Prerequisites

- Rust and Cargo installed
- Linux environment (though it may work on other platforms)

## Installation

1. Clone this repository:
   ```bash
   git clone https://github.com/yourusername/personal-cron.git
   cd personal-cron
   ```

2. Build the project:
   ```bash
   cargo build --release
   ```

3. The compiled binary will be available at `target/release/personal-cron`

## Usage

### Basic Commands

```bash
# Run the scheduler daemon
./target/release/personal-cron run

# Show the monitor UI
./target/release/personal-cron monitor

# List all jobs
./target/release/personal-cron list

# Create a new job template
./target/release/personal-cron create --id daily_backup --name "Daily Backup" --cron "0 0 * * *" --endpoint "https://api.example.com/backup" --method POST
```

### Configuration

The application uses a `config.toml` file for configuration:

```toml
jobs_directory = "cron_jobs"
logs_directory = "logs"
check_interval_ms = 1000
log_level = "info"
monitor_enabled = true
monitor_update_interval_ms = 1000
```

### Job Configuration

Jobs are defined as TOML files in the `cron_jobs` directory. Here's an example:

```toml
id = "weather_check"
name = "Weather API Check"
description = "Checks the weather API every hour"
cron_expression = "0 * * * *"
endpoint = "https://api.example.com/weather"
method = "GET"
enabled = true

[payload]
headers = { "Content-Type" = "application/json", "Authorization" = "Bearer YOUR_API_KEY" }
```

## CRON Expression Format

The application supports standard CRON expressions:

```
┌───────────── minute (0 - 59)
│ ┌───────────── hour (0 - 23)
│ │ ┌───────────── day of month (1 - 31)
│ │ │ ┌───────────── month (1 - 12)
│ │ │ │ ┌───────────── day of week (0 - 6) (Sunday to Saturday)
│ │ │ │ │
│ │ │ │ │
* * * * *
```

Examples:
- `* * * * *` - Every minute
- `0 * * * *` - Every hour at minute 0
- `0 0 * * *` - Every day at midnight
- `0 0 * * 0` - Every Sunday at midnight
- `0 0 1 * *` - First day of each month at midnight

## Monitor UI

The monitor UI provides a terminal-based interface to view and manage your jobs:

- **Jobs tab**: Shows a list of all jobs with their status and next run time
- **Job Details tab**: Shows detailed information about a selected job
- **Logs tab**: Shows application logs

### Keyboard Shortcuts

- `Tab`: Switch between tabs
- `Up/Down`: Navigate through lists
- `r`: Reload job configurations
- `q`: Quit the monitor

## Logging

Logs are stored in the `logs` directory:
- `logs/app.log`: Application logs
- `logs/jobs/{job_id}.log`: Per-job execution logs

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.