//! Everything that changes a server goes through a [`Plan`]: a titled list of
//! shell steps with a risk level. The app shows the plan, asks for
//! confirmation, runs it with a live log and writes the outcome to the audit
//! log. Agents (MCP/REST) get exactly the same plans.

pub mod actions;
pub mod site;

use anyhow::{Result, bail};
use sesame_core::ops::Risk;
use sesame_ssh::Run;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub title: String,
    /// POSIX sh, run as root with `set -eu`. May contain secrets (passwords
    /// in heredocs): it is sent over stdin and never stored or logged.
    pub script: String,
    /// When set, this step's stdout is a secret: it is neither streamed nor
    /// logged, but returned once under this name (e.g. a generated password).
    pub capture_as: Option<String>,
}

impl Step {
    pub fn new(title: impl Into<String>, script: impl Into<String>) -> Self {
        Self { title: title.into(), script: script.into(), capture_as: None }
    }

    pub fn capture(mut self, name: impl Into<String>) -> Self {
        self.capture_as = Some(name.into());
        self
    }
}

/// What a successful run hands back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Last lines of output, for the audit log. Never includes captured secrets.
    pub tail: String,
    /// Secrets to show once: the plan's own plus captured step outputs.
    pub reveal: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Operation name from `sesame_core::ops`.
    pub op: &'static str,
    pub title: String,
    pub risk: Risk,
    /// Plain-language list of what will happen, shown before confirming.
    pub details: Vec<String>,
    /// For destructive plans: the word the user must type to confirm.
    pub confirm_word: Option<String>,
    pub steps: Vec<Step>,
    /// Secrets the user must see once after success (e.g. a new DB password).
    pub reveal: Vec<(String, String)>,
    /// Where revealed secrets are stored: `None` for server-wide ones
    /// (`REDIS_PASSWORD` of the server's Redis), a site domain for site ones,
    /// so a site's credentials never overwrite the server's.
    pub secret_scope: Option<String>,
}

impl Plan {
    /// Vault name for a revealed secret of this plan.
    pub fn secret_name(&self, key: &str) -> String {
        match &self.secret_scope {
            Some(scope) => format!("{scope}/{key}"),
            None => key.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Step { index: usize, title: String },
    Line(String),
    Done { ok: bool, error: Option<String> },
}

/// Prepended to every step: each step runs in a fresh shell.
pub const PREAMBLE: &str = "set -eu\nexport DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=a\n[ -r /etc/os-release ] && . /etc/os-release\n";

/// Runs the plan's steps in order, stopping at the first failure.
pub async fn execute<R: Run>(runner: &R, plan: &Plan, events: Option<UnboundedSender<Event>>) -> Result<Outcome> {
    let send = |e: Event| {
        if let Some(tx) = &events {
            let _ = tx.send(e);
        }
    };
    let mut tail: Vec<String> = Vec::new();
    let mut reveal = plan.reveal.clone();
    for (index, step) in plan.steps.iter().enumerate() {
        send(Event::Step { index, title: step.title.clone() });
        if let Some(name) = &step.capture_as {
            let script = format!("{PREAMBLE}{}\n", step.script);
            match runner.run_script(&script, None).await {
                Ok(o) if o.success() => {
                    // Empty output means "nothing new to reveal" (e.g. existing data kept its users).
                    let value = o.stdout.trim();
                    if !value.is_empty() {
                        reveal.push((name.clone(), value.to_string()));
                    }
                    continue;
                }
                Ok(o) => {
                    let error = format!("step “{}” exited with {}", step.title, o.exit_code.map_or("?".into(), |c| c.to_string()));
                    send(Event::Done { ok: false, error: Some(error.clone()) });
                    bail!(error);
                }
                Err(e) => {
                    let error = format!("step “{}” failed: {e:#}", step.title);
                    send(Event::Done { ok: false, error: Some(error.clone()) });
                    bail!(error);
                }
            }
        }
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let script = format!("{PREAMBLE}{}\n", step.script);
        let run = runner.run_script(&script, Some(tx));
        tokio::pin!(run);
        let out = loop {
            tokio::select! {
                Some(line) = rx.recv() => {
                    tail.push(line.clone());
                    if tail.len() > 60 { tail.remove(0); }
                    send(Event::Line(line));
                }
                out = &mut run => break out,
            }
        };
        while let Ok(line) = rx.try_recv() {
            tail.push(line.clone());
            send(Event::Line(line));
        }
        let failed = match out {
            Ok(o) if o.success() => None,
            Ok(o) => Some(format!("step “{}” exited with {}", step.title, o.exit_code.map_or("?".into(), |c| c.to_string()))),
            Err(e) => Some(format!("step “{}” failed: {e:#}", step.title)),
        };
        if let Some(error) = failed {
            send(Event::Done { ok: false, error: Some(error.clone()) });
            bail!(error);
        }
    }
    send(Event::Done { ok: true, error: None });
    Ok(Outcome { tail: tail.join("\n"), reveal })
}

/// 32 characters from [A-Za-z0-9] — safe inside SQL and shell quotes.
pub fn generate_password() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("system randomness");
    bytes.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use sesame_ssh::CommandOutput;

    use super::*;

    struct Fake {
        scripts: Mutex<Vec<String>>,
        fail_on: Option<usize>,
    }

    impl Run for Fake {
        async fn run_script(&self, script: &str, lines: Option<UnboundedSender<String>>) -> Result<CommandOutput> {
            let n = {
                let mut s = self.scripts.lock().unwrap();
                s.push(script.to_string());
                s.len() - 1
            };
            if let Some(tx) = lines {
                let _ = tx.send(format!("output of step {n}"));
            }
            let code = if self.fail_on == Some(n) { 1 } else { 0 };
            Ok(CommandOutput { exit_code: Some(code), ..Default::default() })
        }
    }

    fn plan(steps: usize) -> Plan {
        Plan {
            op: "test",
            title: "test".into(),
            risk: Risk::Change,
            details: vec![],
            confirm_word: None,
            steps: (0..steps).map(|i| Step::new(format!("s{i}"), format!("echo {i}"))).collect(),
            reveal: vec![],
            secret_scope: None,
        }
    }

    #[tokio::test]
    async fn runs_steps_in_order_with_strict_shell() {
        let fake = Fake { scripts: Mutex::new(vec![]), fail_on: None };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let out = execute(&fake, &plan(2), Some(tx)).await.unwrap();
        assert_eq!(out.tail, "output of step 0\noutput of step 1");
        let scripts = fake.scripts.lock().unwrap();
        assert!(scripts[0].starts_with("set -eu\n"));
        assert!(scripts[1].contains(". /etc/os-release"), "every step sees the OS variables");
        let mut events = vec![];
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        assert_eq!(events.last(), Some(&Event::Done { ok: true, error: None }));
        assert!(events.contains(&Event::Step { index: 1, title: "s1".into() }));
    }

    #[tokio::test]
    async fn stops_at_first_failure() {
        let fake = Fake { scripts: Mutex::new(vec![]), fail_on: Some(0) };
        let err = execute(&fake, &plan(3), None).await.unwrap_err();
        assert!(err.to_string().contains("s0"));
        assert_eq!(fake.scripts.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn captured_output_is_revealed_not_logged() {
        struct Secret;
        impl Run for Secret {
            async fn run_script(&self, _: &str, lines: Option<UnboundedSender<String>>) -> Result<CommandOutput> {
                assert!(lines.is_none(), "secret steps must not stream");
                Ok(CommandOutput { stdout: "s3cr3t\n".into(), exit_code: Some(0), ..Default::default() })
            }
        }
        let mut p = plan(0);
        p.steps.push(Step::new("make password", "openssl rand -hex 16").capture("ADMIN_PASSWORD"));
        let out = execute(&Secret, &p, None).await.unwrap();
        assert_eq!(out.reveal, vec![("ADMIN_PASSWORD".to_string(), "s3cr3t".to_string())]);
        assert!(!out.tail.contains("s3cr3t"));
    }

    #[test]
    fn passwords_are_long_and_plain() {
        let p = generate_password();
        assert_eq!(p.len(), 32);
        assert!(p.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(p, generate_password());
    }
}
