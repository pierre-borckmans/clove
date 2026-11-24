//! Runs warm/setup step commands, streaming their output as events and to a log.

use crate::config::{Exec, Step};
use crate::detect::wrap;
use crate::events::{Event, Reporter};
use anyhow::{Result, bail};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

pub struct StepRun<'a> {
    pub label: &'a str,
    pub command: &'a str,
    pub cwd: &'a Path,
    pub exec: Exec,
    pub step: &'a Step,
    pub env: &'a BTreeMap<String, String>,
    pub log: &'a Path,
}

/// Runs one step command. On failure the error carries the last lines of output
/// and the log path.
pub fn run_step(r: &dyn Reporter, run: StepRun<'_>) -> Result<()> {
    let argv = if run.step.wrap {
        wrap(run.exec, run.command)
    } else {
        wrap(Exec::None, run.command)
    };
    if let Some(p) = run.log.parent() {
        std::fs::create_dir_all(p)?;
    }
    let log = Mutex::new(std::fs::File::create(run.log)?);
    writeln!(log.lock().unwrap(), "$ {}", run.command)?;

    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(run.cwd)
        .envs(run.env)
        .envs(&run.step.env)
        .env("CI", std::env::var("CI").unwrap_or_else(|_| "true".into()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("{}: cannot start {}: {e}", run.label, argv[0]))?;

    let tail = Mutex::new(VecDeque::<String>::new());
    let streams: [Box<dyn std::io::Read + Send>; 2] = [
        Box::new(child.stdout.take().expect("piped")),
        Box::new(child.stderr.take().expect("piped")),
    ];
    std::thread::scope(|s| {
        for stream in streams {
            let (log, tail) = (&log, &tail);
            s.spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let _ = writeln!(log.lock().unwrap(), "{line}");
                    {
                        let mut t = tail.lock().unwrap();
                        t.push_back(line.clone());
                        if t.len() > 20 {
                            t.pop_front();
                        }
                    }
                    r.event(Event::Output {
                        step: run.label.to_string(),
                        line,
                    });
                }
            });
        }
    });
    let status = child.wait()?;

    if !status.success() {
        let t = tail.lock().unwrap();
        let last: Vec<_> = t.iter().rev().take(8).rev().cloned().collect();
        bail!(
            "{} failed ({status}): {}\n{}\nfull log: {}",
            run.label,
            run.command,
            last.join("\n"),
            run.log.display()
        );
    }
    Ok(())
}
