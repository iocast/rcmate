// rcmate - Rclone synchronization automation tool.
// Copyright (c) [year] [your name]
//
// Licensed under the Apache License, Version 2.0 or the MIT license, at your option.
// This file may not be copied, modified, or distributed except according to those terms.

use chrono::Local;
use clap::Parser;
use color_eyre::Result;
use color_eyre::eyre::eyre;
use crossterm::style::Stylize;
use std::fs;
use std::path::PathBuf;
use tracing::Level;
use tracing_appender::non_blocking;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

pub mod app;
pub mod config;
pub mod event;
pub mod rclone_request;
pub mod tui;
pub mod views;

use crate::app::App;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Rclone synchronization automation tool.",
    long_about = None,
    disable_version_flag = true // Disable default -V so we can use -v
)]
struct Args {
    #[arg(long, default_value = "~/.local/share/rcmate/config.toml")]
    config: PathBuf,

    #[arg(
        short = 'v',
        long = "version",
        help = "Print version information (same as Shift+Alt+I in TUI)"
    )]
    show_version: bool,

    #[arg(long, default_value = "rclone")]
    rclone: String,

    #[arg(long)]
    workdir: Option<PathBuf>,

    #[arg(long)]
    log_path: Option<PathBuf>,

    #[arg(
        long,
        default_value = "info",
        help = "Log level (trace, debug, info, warn, error)"
    )]
    log_level: String,

    #[arg(long)]
    rclone_config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install the default panic and error report hooks
    color_eyre::install()?;

    let args = Args::parse();

    // load config file
    let app = App::from_file(&app::expand_tilde(args.config))?;

    // -------------------------------------------------
    // Handle -v / --version flag
    if args.show_version {
        // ASCII Art (Cyan, Bold)
        println!("{}", "  _ __   ___ _ __ ___   __ _| |_ ___ ".cyan().bold());
        println!(
            "{}",
            " | '__| / __| '_ ` _ \\ / _` | __/ _ \\".cyan().bold()
        );
        println!("{}", " | |   | (__| | | | | | (_| | ||  __/".cyan().bold());
        println!(
            "{}",
            " |_|    \\___|_| |_| |_|\\__,_|\\__\\___|".cyan().bold()
        );
        println!();

        let rclone_bin = {
            let rclone = app.rclone.read().await;
            rclone.bin.clone()
        };

        let output = tokio::process::Command::new(&rclone_bin)
            .arg("version")
            .output()
            .await;

        let rclone_version = match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if let Some(first_line) = stdout.lines().next() {
                    first_line.replace("rclone ", "").to_string()
                } else {
                    stdout.trim().to_string()
                }
            }
            _ => "unable to determine".to_string(),
        };

        // Labels (White, Bold) + Values (Yellow, Bold / DarkGrey)
        println!(
            "{}{}",
            "rcmate version:   ".white().bold(),
            env!("CARGO_PKG_VERSION").yellow().bold()
        );
        println!(
            "{}{}",
            "rclone version:   ".white().bold(),
            rclone_version.yellow().bold()
        );
        println!(
            "{}{}",
            "rclone binary:    ".white().bold(),
            rclone_bin.dark_grey()
        );
        println!(
            "{}{}",
            "config path:      ".white().bold(),
            app.config_path.display().to_string().dark_grey()
        );
        println!();

        // Tagline (DarkGrey)
        println!("{}", "Your Friendly Rclone Companion".dark_grey());

        return Ok(());
    }

    // -------------------------------------------------
    // set/overwrite config values from args
    {
        let mut rclone = app.rclone.write().await;
        let mut general = app.general.write().await;

        // rclone binary (Directly assign since it's now a String)
        rclone.bin = args.rclone.clone();

        // rclone workdir
        if let Some(ref workdir) = args.workdir {
            rclone.workdir = Some(app::expand_tilde(workdir.to_path_buf()));
        } else if let Some(ref workdir) = rclone.workdir {
            rclone.workdir = Some(app::expand_tilde(workdir.to_path_buf()));
        }

        // rclone config
        if let Some(ref rclone_config) = args.rclone_config {
            rclone.config = Some(app::expand_tilde(rclone_config.to_path_buf()));
        } else if let Some(ref rclone_config) = rclone.config {
            rclone.config = Some(app::expand_tilde(rclone_config.to_path_buf()));
        }

        // log_path
        if let Some(ref log) = args.log_path {
            general.log_path = Some(app::expand_tilde(log.to_path_buf()));
        }

        // log_level
        general.log_level = args.log_level.clone();

        if let Some(ref log) = general.log_path {
            let mut log_path = app::expand_tilde(log.to_path_buf());

            if log_path.is_dir() {
                let rfilename = format!(
                    "rclone_{}.log",
                    Local::now().format("%Y%m%d_%H%M%S").to_string()
                );

                log_path = app::expand_tilde(log.to_path_buf()).join(rfilename);
            }

            general.log_path = Some(log_path);
        }
    }

    // -------------------------------------------------
    // init logging
    let log_level = app.general.read().await.log_level.clone();
    let _log_guard = init_tracing(&app.general.read().await.log_path, &log_level)?;

    // -------------------------------------------------
    // run app
    // if args.tui {
    let terminal = ratatui::init();
    let _result = app.run(terminal).await?;
    ratatui::restore();
    // } else {
    //     let _ = app.sync().await;
    // }

    Ok(())
}

/// Initialize the tracing subscriber to log to a file
///
/// This function initializes the tracing subscriber to log to a file named `tracing.log` in the
/// current directory. The function returns a [`WorkerGuard`] that must be kept alive for the
/// duration of the program to ensure that logs are flushed to the file on shutdown. The logs are
/// written in a non-blocking fashion to ensure that the logs do not block the main thread.
fn init_tracing(log_file: &Option<PathBuf>, log_level: &str) -> Result<Option<WorkerGuard>> {
    let mut guard = None;

    if let Some(log) = log_file {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .map_err(|e| eyre!("Error opening log file {}: {}", log.display(), e))?;

        let (non_blocking, worker_guard) = non_blocking(file);
        guard = Some(worker_guard);

        // Parse the log level string into a tracing Level
        let level = match log_level.to_lowercase().as_str() {
            "trace" => Level::TRACE,
            "debug" => Level::DEBUG,
            "info" => Level::INFO,
            "warn" | "warning" => Level::WARN,
            "error" => Level::ERROR,
            _ => Level::INFO, // Fallback to INFO if invalid string is provided
        };

        // By default, the subscriber is configured to log all events with the specified level,
        // but this can be changed by setting the `RUST_LOG` environment variable.
        let env_filter = EnvFilter::builder()
            .with_default_directive(level.into())
            .from_env_lossy();

        let subscriber = tracing_subscriber::fmt::Subscriber::builder()
            .with_writer(non_blocking)
            .with_env_filter(env_filter)
            .finish();

        // Set the subscriber as the global default for the application
        tracing::subscriber::set_global_default(subscriber)
            .expect("Failed to set global subscriber");
    }

    Ok(guard)
}
