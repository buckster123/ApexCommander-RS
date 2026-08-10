//! `apex-commander` — human/ops CLI face.
//!
//! Thin over the core library: parse arguments, call one function, render the result.
//! No domain logic lives here.

use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use apex_commander::a11y::{
    activate, click_element, do_action, find_elements, focused_element, frontmost, list_apps,
    list_windows, set_value, snapshot, type_into, AtspiSession,
};
use apex_commander::capture::screenshot;
use apex_commander::doctor::run_doctor;
use apex_commander::error::HarnessError;
use apex_commander::field::{markdown_summary, run_field_report};
use apex_commander::input::{key, mouse_click, mouse_move, type_text};
use apex_commander::launch::launch_app;
use apex_commander::selftest::{run_selftest, SelftestOpts};
use apex_commander::types::{FindQuery, SnapshotOpts, TargetRef};
use apex_commander::wait::{wait_for_element, wait_for_stable, wait_ms};
use apex_commander::{NAME, VERSION};

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
    /// Structured readiness report.
    Doctor,
    /// List AT-SPI application roots.
    ListApps,
    /// List top-level windows (frames/dialogs).
    ListWindows,
    /// Best-effort frontmost / focused window.
    Frontmost,
    /// Focus / raise a window (AT-SPI GrabFocus).
    Activate {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
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
        #[arg(long)]
        frontmost: bool,
        #[arg(long, default_value_t = 6)]
        max_depth: u32,
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
        #[arg(long)]
        role: Option<String>,
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
    /// AT-SPI DoAction on an element (prefer over coordinate click).
    DoAction {
        /// Element id `{bus}|{path}`.
        #[arg(long)]
        id: String,
        /// Action name (default: Click/Press/Activate or first).
        #[arg(long)]
        action: Option<String>,
        /// Explicit action index.
        #[arg(long)]
        index: Option<i32>,
    },
    /// Click via AT-SPI (Click action).
    Click {
        #[arg(long)]
        id: String,
    },
    /// Set text via EditableText (prefer over type-text).
    TypeInto {
        #[arg(long)]
        id: String,
        /// Text to set (or append with --append).
        text: String,
        #[arg(long)]
        append: bool,
    },
    /// Set a Value interface (slider/spin).
    SetValue {
        #[arg(long)]
        id: String,
        value: f64,
    },
    /// Screenshot (portal / shell / grim). Optional window target for crop.
    Screenshot {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        pid: Option<u32>,
        #[arg(long)]
        frontmost: bool,
        /// Full display even if a target is given.
        #[arg(long)]
        full: bool,
    },
    /// Move pointer (ydotool/xdotool fallback).
    MouseMove { x: i32, y: i32 },
    /// Click at coordinates (fallback; prefer do-action).
    MouseClick {
        x: i32,
        y: i32,
        #[arg(long, default_value_t = 1)]
        button: u8,
    },
    /// Type via real keys (fallback; prefer type-into).
    TypeText { text: String },
    /// Key combo (backend-specific, e.g. Return or ctrl+c).
    Key { keyspec: String },
    /// Launch an app by desktop id (org.gnome.Calculator) or executable.
    Launch {
        /// Desktop file id or executable path/name.
        target: String,
    },
    /// Sleep for N milliseconds.
    Wait {
        #[arg(long, default_value_t = 1000)]
        ms: u64,
    },
    /// Poll until an element matches under a target window.
    WaitForElement {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        pid: Option<u32>,
        #[arg(long)]
        frontmost: bool,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        element_name: Option<String>,
        #[arg(long)]
        text: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long, default_value_t = 10000)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 200)]
        poll_ms: u64,
    },
    /// Poll until the a11y tree fingerprint stays stable.
    WaitForStable {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        pid: Option<u32>,
        #[arg(long)]
        frontmost: bool,
        #[arg(long, default_value_t = 10000)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 200)]
        poll_ms: u64,
        #[arg(long, default_value_t = 400)]
        stable_for_ms: u64,
    },
    /// Structured smoke suite (doctor + discovery + snapshot). Use --confirm for mouse wiggle.
    Selftest {
        /// Allow mutating steps (visible mouse wiggle when input backend exists).
        #[arg(long)]
        confirm: bool,
        /// Preferred window/app name for snapshot step.
        #[arg(long)]
        target: Option<String>,
    },
    /// Compositor field matrix (GNOME / Plasma / Hyprland). Emits JSON; --markdown for docs paste.
    FieldReport {
        /// Also print a markdown section to stderr for docs/field-matrix.md.
        #[arg(long)]
        markdown: bool,
        /// Reserved for future mutating field checks.
        #[arg(long)]
        confirm: bool,
    },
    /// Print version.
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
            emit(&list_apps(&session).await?, cli.json)?;
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
            emit(&frontmost(&session).await?, cli.json)?;
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
            emit(
                &find_elements(&session, &target, query, SnapshotOpts::default()).await?,
                cli.json,
            )?;
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
            emit(&focused_element(&session, target.as_ref()).await?, cli.json)?;
        }
        Command::DoAction { id, action, index } => {
            let session = AtspiSession::connect().await?;
            let result = do_action(&session, &id, action.as_deref(), index).await?;
            emit(&result, cli.json)?;
            if !result.ok {
                std::process::exit(1);
            }
        }
        Command::Click { id } => {
            let session = AtspiSession::connect().await?;
            let result = click_element(&session, &id).await?;
            emit(&result, cli.json)?;
            if !result.ok {
                std::process::exit(1);
            }
        }
        Command::TypeInto { id, text, append } => {
            let session = AtspiSession::connect().await?;
            let result = type_into(&session, &id, &text, append).await?;
            emit(&result, cli.json)?;
            if !result.ok {
                std::process::exit(1);
            }
        }
        Command::SetValue { id, value } => {
            let session = AtspiSession::connect().await?;
            emit(&set_value(&session, &id, value).await?, cli.json)?;
        }
        Command::Screenshot {
            id,
            name,
            pid,
            frontmost: fm,
            full,
        } => {
            let session = AtspiSession::connect().await.ok();
            let target = if full {
                None
            } else if id.is_some() || name.is_some() || pid.is_some() || fm {
                Some(target_from_flags(id, name, pid, fm)?)
            } else {
                None
            };
            let result = screenshot(session.as_ref(), target.as_ref()).await?;
            emit(&result, cli.json)?;
        }
        Command::MouseMove { x, y } => {
            let detail = mouse_move(x, y).await?;
            if cli.json {
                println!("{}", serde_json::json!({ "ok": true, "detail": detail }));
            } else {
                println!("{detail}");
            }
        }
        Command::MouseClick { x, y, button } => {
            let detail = mouse_click(x, y, button).await?;
            if cli.json {
                println!("{}", serde_json::json!({ "ok": true, "detail": detail }));
            } else {
                println!("{detail}");
            }
        }
        Command::TypeText { text } => {
            let detail = type_text(&text).await?;
            if cli.json {
                println!("{}", serde_json::json!({ "ok": true, "detail": detail }));
            } else {
                println!("{detail}");
            }
        }
        Command::Key { keyspec } => {
            let detail = key(&keyspec).await?;
            if cli.json {
                println!("{}", serde_json::json!({ "ok": true, "detail": detail }));
            } else {
                println!("{detail}");
            }
        }
        Command::Launch { target } => {
            let result = launch_app(&target).await?;
            emit(&result, cli.json)?;
            if !result.ok {
                std::process::exit(1);
            }
        }
        Command::Wait { ms } => {
            emit(&wait_ms(Some(ms)).await, cli.json)?;
        }
        Command::WaitForElement {
            id,
            name,
            pid,
            frontmost: fm,
            role,
            element_name,
            text,
            state,
            timeout_ms,
            poll_ms,
        } => {
            let session = AtspiSession::connect().await?;
            let target = target_from_flags(id, name, pid, fm)?;
            let query = FindQuery {
                role,
                name: element_name,
                name_exact: false,
                text,
                state,
                description: None,
                max_results: 5,
            };
            let result =
                wait_for_element(&session, &target, query, Some(timeout_ms), Some(poll_ms)).await?;
            emit(&result, cli.json)?;
            if !result.ok {
                std::process::exit(3);
            }
        }
        Command::WaitForStable {
            id,
            name,
            pid,
            frontmost: fm,
            timeout_ms,
            poll_ms,
            stable_for_ms,
        } => {
            let session = AtspiSession::connect().await?;
            let target = target_from_flags(id, name, pid, fm)?;
            let result = wait_for_stable(
                &session,
                &target,
                Some(timeout_ms),
                Some(poll_ms),
                Some(stable_for_ms),
            )
            .await?;
            emit(&result, cli.json)?;
            if !result.ok {
                std::process::exit(3);
            }
        }
        Command::Selftest { confirm, target } => {
            let report = run_selftest(SelftestOpts {
                confirm_mutate: confirm,
                target_name: target,
            })
            .await?;
            emit(&report, cli.json)?;
            if !report.ok {
                std::process::exit(2);
            }
        }
        Command::FieldReport { markdown, confirm } => {
            let report = run_field_report(confirm).await?;
            if markdown {
                eprintln!("{}", markdown_summary(&report));
            }
            // Always machine-readable on stdout for the matrix ledger.
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.ok {
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

fn emit<T: serde::Serialize>(value: &T, _json: bool) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_tree(node: &apex_commander::types::A11yNode, indent: usize) {
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
