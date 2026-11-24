//! Long operations (init, refresh, new, remove, deinit) run on their own
//! thread and stream clove's progress events to every window.

use crate::commands::{CmdResult, err};
use clove_core::{Clove, Event, NewOptions, RemoveOptions};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// Runs blocking work off the async runtime and flattens the error for the frontend.
pub async fn block<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpRequest {
    Init {
        root: PathBuf,
    },
    Refresh {
        root: PathBuf,
    },
    New {
        root: PathBuf,
        branch: Option<String>,
        base: Option<String>,
        name: Option<String>,
        #[serde(default)]
        no_setup: bool,
        #[serde(default)]
        detach: bool,
    },
    Remove {
        root: PathBuf,
        name: String,
        #[serde(default)]
        force: bool,
        #[serde(default)]
        delete_branch: bool,
    },
    Deinit {
        root: PathBuf,
    },
}

#[derive(Clone, Serialize)]
struct OpStarted {
    id: String,
    at: u64,
    op: OpRequest,
}

#[derive(Clone, Serialize)]
struct OpEvent {
    id: String,
    /// Milliseconds since the Unix epoch, so the UI can lay steps on a timeline.
    at: u64,
    event: Event,
}

#[derive(Clone, Serialize)]
struct OpFinished {
    id: String,
    at: u64,
    ok: bool,
    error: Option<String>,
    result: serde_json::Value,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

static NEXT: AtomicU64 = AtomicU64::new(1);

/// Starts an operation and returns its id at once. Every window hears
/// `op-started`, then `op-event` for progress and `op-finished` for the outcome.
#[tauri::command]
pub fn start_op(app: AppHandle, op: OpRequest) -> CmdResult<String> {
    let id = format!("op-{}-{}", now_ms(), NEXT.fetch_add(1, Ordering::Relaxed));
    let thread_id = id.clone();
    let _ = app.emit(
        "op-started",
        OpStarted {
            id: id.clone(),
            at: now_ms(),
            op: op.clone(),
        },
    );
    std::thread::Builder::new()
        .name(format!("clove-{id}"))
        .spawn(move || {
            let id = thread_id;
            let events_app = app.clone();
            let events_id = id.clone();
            let reporter = move |event: Event| {
                let _ = events_app.emit(
                    "op-event",
                    OpEvent {
                        id: events_id.clone(),
                        at: now_ms(),
                        event,
                    },
                );
            };
            let res = run(&op, &reporter);
            let (ok, error, result) = match res {
                Ok(v) => (true, None, v),
                Err(e) => (false, Some(err(e)), serde_json::Value::Null),
            };
            let _ = app.emit(
                "op-finished",
                OpFinished {
                    id,
                    at: now_ms(),
                    ok,
                    error,
                    result,
                },
            );
        })
        .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Runs one operation to completion on the calling thread.
pub fn run(op: &OpRequest, r: &dyn clove_core::Reporter) -> anyhow::Result<serde_json::Value> {
    Ok(match op {
        OpRequest::Init { root } => serde_json::to_value(Clove::discover(root)?.init(r)?)?,
        OpRequest::Refresh { root } => serde_json::to_value(Clove::open(root)?.refresh(r)?)?,
        OpRequest::New {
            root,
            branch,
            base,
            name,
            no_setup,
            detach,
        } => {
            let opts = NewOptions {
                branch: branch.clone().filter(|s| !s.trim().is_empty()),
                base: base.clone().filter(|s| !s.trim().is_empty()),
                name: name.clone().filter(|s| !s.trim().is_empty()),
                detach: *detach,
                no_setup: *no_setup,
                fetch: None,
            };
            serde_json::to_value(Clove::open(root)?.new_clone(&opts, r)?)?
        }
        OpRequest::Remove {
            root,
            name,
            force,
            delete_branch,
        } => {
            let opts = RemoveOptions {
                force: *force,
                delete_branch: *delete_branch,
            };
            Clove::open(root)?.remove(name, &opts, r)?;
            serde_json::Value::Null
        }
        OpRequest::Deinit { root } => {
            Clove::open(root)?.deinit(r)?;
            serde_json::Value::Null
        }
    })
}
