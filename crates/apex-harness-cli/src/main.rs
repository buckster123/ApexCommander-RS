//! `apex-harness` — human/ops CLI face.
//!
//! Thin over the core library: parse arguments, call one function, render the result.
//! No domain logic lives here.

use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use apex_harness::a11y::{
    activate, find_elements, focused_element, frontmost, list_apps, list_windows, snapshot,
    AtspiSession,
};
use apex_harness::doctor::run_doctor;
use apex_harness::error::HarnessError;
use apex_harness::types::{FindQuery, SnapshotOpts, TargetRef};
use apex_harness::{NAME, VERSION};

#[derive(Parser)]
#[command(
    name = NAME,
    version = VERSION,
    about = "Agent hands for Linux — AT-SPI-first desktop control"
)]
struct Cli {
    /// Emit machine-readable JSON on stdout.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Structured readiness report (session, AT-SPI, input, capture, recommendations).
    Doctor,
    /// List AT-SPI application roots.
    ListApps,
    /// List top-level windows (frames/dialogs).
    ListWindows,
    /// Best-effort frontmost / focused window.
    Frontmost,
    /// Focus / raise a window (AT-SPI GrabFocus).
    Activate {
        /// Exact `{bus}|{path}` id.
        #[arg(long)]
        id: Option<String>,
        /// Case-insensitive substring of window title or app name.
        #[arg(long)]
        name: Option<String>,
        /// Process id.
        #[arg(long)]
        pid: Option<u32>,
    },
    /// Compact accessibility tree of a window/app.
    Snapshot {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        pid: Option<u32>,
        /// Use frontmost window.
        #[arg(long)]
        frontmost: bool,
        /// Max tree depth (default 6).
        #[arg(long, default_value_t = 6)]
        max_depth: u32,
        /// Max nodes (default 200).
        #[arg(long, default_value_t = 200)]
        max_nodes: u32,
    },
    /// Find elements under a target by role/name/text/state.
    Find {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        pid: Option<u32>,
        #[arg(long)]
        frontmost: bool,
        /// Role filter (e.g. button, frame).
        #[arg(long)]
        role: Option<String>,
        /// Name substring (or exact with --name-exact). Element name, not window name —
        /// use --window-name for the target window.
        #[arg(long)]
        element_name: Option<String>,
        #[arg(long)]
        name_exact: bool,
        #[arg(long)]
        text: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long, default_value_t = 25)]
        max_results: u32,
        /// Window title / app name selector (alias of --name for target).
        #[arg(long)]
        window_name: Option<String>,
    },
    /// Currently focused element under a target (default: frontmost window).
    FocusedElement {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        pid: Option<u32>,
        #[arg(long)]
        frontmost: bool,
    },
    /// Print version (also available as --version).
    Version,
}

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(e) = run().await {
        if let Some(he) = e.downcast_ref::<HarnessError>() {
            eprintln!("error: {he}");
            return ExitCode::from(he.exit_code() as u8);
        }
        eprintln!("error: {e:#}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn run() -> Result<()> {
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
            let report = run_doctor().await;
            emit(&report, cli.json)?;
            if !report.ok {
                std::process::exit(2);
            }
        }
        Command::ListApps => {
            let session = AtspiSession::connect().await?;
            let apps = list_apps(&session).await?;
            emit(&apps, cli.json)?;
        }
        Command::ListWindows => {
            let session = AtspiSession::connect().await?;
            let wins = list_windows(&session).await?;
            if cli.json {
                emit(&wins, true)?;
            } else {
                for w in &wins {
                    let focus = if w.focused { "*" } else { " " };
                    let app = w.app_name.as_deref().unwrap_or("?");
                    println!(
                        "{focus} [{app}] {}  ({})",
                        if w.title.is_empty() {
                            "(no title)"
                        } else {
                            &w.title
                        },
                        w.id
                    );
                }
                println!("{} window(s)", wins.len());
            }
        }
        Command::Frontmost => {
            let session = AtspiSession::connect().await?;
            let w = frontmost(&session).await?;
            emit(&w, cli.json)?;
        }
        Command::Activate { id, name, pid } => {
            let session = AtspiSession::connect().await?;
            let target = target_from_flags(id, name, pid, false)?;
            let result = activate(&session, &target).await?;
            emit(&result, cli.json)?;
            if !result.ok {
                std::process::exit(1);
            }
        }
        Command::Snapshot {
            id,
            name,
            pid,
            frontmost: fm,
            max_depth,
            max_nodes,
        } => {
            let session = AtspiSession::connect().await?;
            let target = target_from_flags(id, name, pid, fm)?;
            let opts = SnapshotOpts {
                max_depth,
                max_nodes,
                include_bounds: true,
                include_actions: true,
            };
            let (tree, stats) = snapshot(&session, &target, opts).await?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "tree": tree,
                        "stats": stats,
                    }))?
                );
            } else {
                println!(
                    "nodes={} truncated={} max_depth_hit={}",
                    stats.nodes_emitted, stats.truncated, stats.max_depth_hit
                );
                print_tree(&tree, 0);
            }
        }
        Command::Find {
            id,
            name,
            pid,
            frontmost: fm,
            role,
            element_name,
            name_exact,
            text,
            state,
            max_results,
            window_name,
        } => {
            let session = AtspiSession::connect().await?;
            let win_name = window_name.or(name);
            let target = target_from_flags(id, win_name, pid, fm)?;
            let query = FindQuery {
                role,
                name: element_name,
                name_exact,
                text,
                state,
                description: None,
                max_results,
            };
            let hits = find_elements(&session, &target, query, SnapshotOpts::default()).await?;
            emit(&hits, cli.json)?;
        }
        Command::FocusedElement {
            id,
            name,
            pid,
            frontmost: fm,
        } => {
            let session = AtspiSession::connect().await?;
            let target = if id.is_none() && name.is_none() && pid.is_none() && !fm {
                None
            } else {
                Some(target_from_flags(id, name, pid, fm)?)
            };
            let hit = focused_element(&session, target.as_ref()).await?;
            emit(&hit, cli.json)?;
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

fn target_from_flags(
    id: Option<String>,
    name: Option<String>,
    pid: Option<u32>,
    frontmost: bool,
) -> Result<TargetRef> {
    let set = id.is_some() as u8 + name.is_some() as u8 + pid.is_some() as u8 + u8::from(frontmost);
    if set > 1 {
        bail!("specify only one of --id / --name / --pid / --frontmost");
    }
    if let Some(id) = id {
        return Ok(TargetRef::Id(id));
    }
    if let Some(name) = name {
        return Ok(TargetRef::Name(name));
    }
    if let Some(pid) = pid {
        return Ok(TargetRef::Pid(pid));
    }
    if frontmost {
        return Ok(TargetRef::Frontmost);
    }
    bail!("need a target: --id, --name, --pid, or --frontmost");
}

fn emit<T: serde::Serialize>(value: &T, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        // Human default: pretty JSON still (structured data). List commands may override.
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

fn print_tree(node: &apex_harness::types::A11yNode, indent: usize) {
    let pad = "  ".repeat(indent);
    let name = node
        .name
        .as_deref()
        .map(|n| format!(" {n:?}"))
        .unwrap_or_default();
    let states = if node.states.is_empty() {
        String::new()
    } else {
        format!(" [{}]", node.states.join(","))
    };
    println!("{pad}{}{name}{states}", node.role);
    for c in &node.children {
        print_tree(c, indent + 1);
    }
}
