//! `apex-harness` — human/ops CLI face.
//!
//! Thin over the core library: parse arguments, call one function, render the result.
//! No domain logic lives here.

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use apex_harness::doctor::run_doctor;
use apex_harness::{NAME, VERSION};

#[derive(Parser)]
#[command(
    name = NAME,
    version = VERSION,
    about = "Agent hands for Linux — AT-SPI-first desktop control"
)]
struct Cli {
    /// Emit machine-readable JSON on stdout (default for scripting).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Structured readiness report (session, AT-SPI, input, capture, recommendations).
    Doctor,
    /// Print version (also available as --version).
    Version,
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => {
            let report = run_doctor();
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", report.summary);
                for cap in &report.capabilities {
                    let mark = if cap.available { "ok" } else { "--" };
                    let detail = cap.detail.as_deref().unwrap_or("");
                    println!("  [{mark}] {}  {detail}", cap.name);
                }
                if !report.recommendations.is_empty() {
                    println!("recommendations:");
                    for r in &report.recommendations {
                        println!("  - {r}");
                    }
                }
            }
            if !report.ok {
                // Non-zero so scripts can gate on readiness, but body still printed.
                std::process::exit(2);
            }
        }
        Command::Version => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::json!({ "name": NAME, "version": VERSION })
                );
            } else {
                println!("{NAME} {VERSION}");
            }
        }
    }
    Ok(())
}
