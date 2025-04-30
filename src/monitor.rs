use anyhow::Result;
use chrono::{Utc};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use std::sync::{Arc};
use std::time::Duration;
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Terminal,
};

use crate::logger::LogEntry;
use crate::scheduler::Scheduler;

enum MonitorTab {
    Jobs,
    JobDetails,
    Logs,
}

pub struct Monitor {
    scheduler: Arc<Scheduler>,
    selected_tab: MonitorTab,
    selected_job_index: usize,
}

impl Monitor {
    pub fn new(scheduler: Arc<Scheduler>) -> Self {
        Monitor {
            scheduler,
            selected_tab: MonitorTab::Jobs,
            selected_job_index: 0,
        }
    }

    pub async fn start(&mut self, update_interval_ms: u64) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Run the UI loop
        let res = self.run_app(&mut terminal, update_interval_ms).await;

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        if let Err(err) = res {
            log::error!("Error running monitor: {:?}", err);
        }

        Ok(())
    }

    async fn run_app<B: tui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
        update_interval_ms: u64,
    ) -> Result<()> {
        let mut last_update = Utc::now();

        loop {
            // Render UI
            terminal.draw(|f| self.render_ui(f))?;

            // Handle input
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Tab => {
                            self.selected_tab = match self.selected_tab {
                                MonitorTab::Jobs => MonitorTab::JobDetails,
                                MonitorTab::JobDetails => MonitorTab::Logs,
                                MonitorTab::Logs => MonitorTab::Jobs,
                            };
                        }
                        KeyCode::Up => {
                            if self.selected_job_index > 0 {
                                self.selected_job_index -= 1;
                            }
                        }
                        KeyCode::Down => {
                            let jobs = self.scheduler.get_jobs();
                            if !jobs.is_empty() && self.selected_job_index < jobs.len() - 1 {
                                self.selected_job_index += 1;
                            }
                        }
                        KeyCode::Char('r') => {
                            let _ = self.scheduler.reload_jobs();
                        }
                        _ => {}
                    }
                }
            }

            // Update data periodically
            let now = Utc::now();
            if (now - last_update).num_milliseconds() > update_interval_ms as i64 {
                last_update = now;
                // No need to explicitly update as we fetch latest data in render_ui
            }
        }
    }

    fn render_ui<B: tui::backend::Backend>(&self, f: &mut tui::Frame<B>) {
        // Create layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Min(0),
                ]
                .as_ref(),
            )
            .split(f.size());

        // Render tabs
        let tabs = Tabs::new(vec![
            Spans::from(vec![Span::styled(
                "Jobs",
                Style::default().fg(Color::White),
            )]),
            Spans::from(vec![Span::styled(
                "Job Details",
                Style::default().fg(Color::White),
            )]),
            Spans::from(vec![Span::styled(
                "Logs",
                Style::default().fg(Color::White),
            )]),
        ])
        .select(match self.selected_tab {
            MonitorTab::Jobs => 0,
            MonitorTab::JobDetails => 1,
            MonitorTab::Logs => 2,
        })
        .block(Block::default().borders(Borders::ALL).title("Navigation"))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

        f.render_widget(tabs, chunks[0]);

        // Render content based on selected tab
        match self.selected_tab {
            MonitorTab::Jobs => self.render_jobs_tab(f, chunks[1]),
            MonitorTab::JobDetails => self.render_job_details_tab(f, chunks[1]),
            MonitorTab::Logs => self.render_logs_tab(f, chunks[1]),
        }
    }

    fn render_jobs_tab<B: tui::backend::Backend>(&self, f: &mut tui::Frame<B>, area: tui::layout::Rect) {
        let jobs = self.scheduler.get_jobs();
        
        let items: Vec<ListItem> = jobs
            .iter()
            .enumerate()
            .map(|(i, job)| {
                let status_style = if let Some(status) = &job.last_status {
                    if status.success {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    }
                } else {
                    Style::default().fg(Color::Gray)
                };

                let next_run = job.next_run.map_or_else(
                    || "Not scheduled".to_string(),
                    |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                );

                let status_text = if let Some(status) = &job.last_status {
                    if status.success {
                        "✓"
                    } else {
                        "✗"
                    }
                } else {
                    "-"
                };

                let content = vec![Spans::from(vec![
                    Span::styled(format!("{} ", status_text), status_style),
                    Span::raw(format!("{:<20} ", job.id)),
                    Span::raw(format!("{:<30} ", job.name)),
                    Span::raw(format!("Next: {}", next_run)),
                ])];

                let style = if i == self.selected_job_index {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(content).style(style)
            })
            .collect();

        let jobs_list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Jobs"))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_widget(jobs_list, area);
    }

    fn render_job_details_tab<B: tui::backend::Backend>(
        &self,
        f: &mut tui::Frame<B>,
        area: tui::layout::Rect,
    ) {
        let jobs = self.scheduler.get_jobs();
        
        if jobs.is_empty() || self.selected_job_index >= jobs.len() {
            let paragraph = Paragraph::new("No job selected")
                .block(Block::default().borders(Borders::ALL).title("Job Details"));
            f.render_widget(paragraph, area);
            return;
        }

        let job = &jobs[self.selected_job_index];

        // Split the area
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(12),
                    Constraint::Min(0),
                ]
                .as_ref(),
            )
            .split(area);

        // Job basic info
        let last_run = job.last_run.map_or_else(
            || "Never".to_string(),
            |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        );

        let next_run = job.next_run.map_or_else(
            || "Not scheduled".to_string(),
            |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        );

		let time_until_next_run = job.next_run.map_or_else(
			|| "N/A".to_string(),
			|dt| {
				let duration = dt.signed_duration_since(Utc::now());
				format!("{} seconds", duration.num_seconds())
			},
		);

        let status_text = if let Some(status) = &job.last_status {
            if status.success {
                "Success".to_string()
            } else {
                format!(
                    "Failed: {}",
                    status.error_message.as_deref().unwrap_or("Unknown error")
                )
            }
        } else {
            "Never run".to_string()
        };

        let status_style = if let Some(status) = &job.last_status {
            if status.success {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            }
        } else {
            Style::default().fg(Color::Gray)
        };

        let basic_info = vec![
            Spans::from(vec![
                Span::raw("ID:            "),
                Span::styled(&job.id, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Spans::from(vec![
                Span::raw("Name:          "),
                Span::raw(&job.name),
            ]),
            Spans::from(vec![
                Span::raw("Description:   "),
                Span::raw(job.description.as_deref().unwrap_or("None")),
            ]),
            Spans::from(vec![
                Span::raw("Cron:          "),
                Span::raw(&job.cron_expression),
            ]),
            Spans::from(vec![
                Span::raw("Endpoint:      "),
                Span::raw(job.endpoint.split('?').next().unwrap_or("").to_string()),
            ]),
            Spans::from(vec![
                Span::raw("Method:        "),
                Span::raw(&job.method),
            ]),
            Spans::from(vec![
                Span::raw("Last Run:      "),
                Span::raw(&last_run),
            ]),
            Spans::from(vec![
                Span::raw("Next Run:      "),
                Span::raw(&next_run),
            ]),
			Spans::from(vec![
				Span::raw("Time Until:    "),
				Span::raw(&time_until_next_run),
			]),
            Spans::from(vec![
                Span::raw("Status:        "),
                Span::styled(&status_text, status_style),
            ]),
        ];

        let job_info = Paragraph::new(basic_info)
            .block(Block::default().borders(Borders::ALL).title("Job Details"));

        f.render_widget(job_info, chunks[0]);

        // Job history
        let history = self.scheduler.get_job_status_history(&job.id).unwrap_or_default();
        let history_items: Vec<ListItem> = history
            .iter()
            .map(|status| {
                let style = if status.success {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                };

                let content = vec![Spans::from(vec![
                    Span::raw(format!(
                        "{} ",
                        status.timestamp.format("%Y-%m-%d %H:%M:%S")
                    )),
                    Span::styled(
                        format!(
                            "{} ({} ms)",
                            if status.success { "Success" } else { "Failed" },
                            status.duration_ms
                        ),
                        style,
                    ),
                    Span::raw(if !status.success {
                        format!(
                            " - {}",
                            status.error_message.as_deref().unwrap_or("Unknown error")
                        )
                    } else {
                        String::new()
                    }),
                ])];

                ListItem::new(content)
            })
            .collect();

        let history_list = List::new(history_items)
            .block(Block::default().borders(Borders::ALL).title("Job History"));

        f.render_widget(history_list, chunks[1]);
    }

    fn render_logs_tab<B: tui::backend::Backend>(&self, f: &mut tui::Frame<B>, area: tui::layout::Rect) {
        // This is a placeholder - in a real implementation, you'd fetch logs from your logger
        let logs = vec![
            LogEntry {
                timestamp: Utc::now(),
                level: "INFO".to_string(),
                message: "Application started".to_string(),
                job_id: None,
            },
            // Add more log entries here
        ];

        let log_items: Vec<ListItem> = logs
            .iter()
            .map(|log| {
                let style = match log.level.as_str() {
                    "ERROR" => Style::default().fg(Color::Red),
                    "WARN" => Style::default().fg(Color::Yellow),
                    "INFO" => Style::default().fg(Color::White),
                    "DEBUG" => Style::default().fg(Color::Blue),
                    "TRACE" => Style::default().fg(Color::Gray),
                    _ => Style::default(),
                };

                let job_id = log.job_id.as_deref().unwrap_or("");
                let content = vec![Spans::from(vec![
                    Span::raw(format!(
                        "{} ",
                        log.timestamp.format("%Y-%m-%d %H:%M:%S")
                    )),
                    Span::styled(format!("[{}] ", log.level), style),
                    Span::raw(if !job_id.is_empty() {
                        format!("[Job {}] ", job_id)
                    } else {
                        String::new()
                    }),
                    Span::raw(&log.message),
                ])];

                ListItem::new(content)
            })
            .collect();

        let logs_list = List::new(log_items)
            .block(Block::default().borders(Borders::ALL).title("Application Logs"));

        f.render_widget(logs_list, area);
    }
}