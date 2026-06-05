//! Manages the Python sidecar process and JSON-RPC communication.

use parking_lot::{Condvar, Mutex};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A running sidecar process that communicates via JSON-RPC over stdio.
pub struct SidecarProcess {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
    writer: BufWriter<std::process::ChildStdin>,
    next_id: AtomicU64,
}

impl SidecarProcess {
    /// Spawn the Python sidecar process.
    pub fn spawn(python: &str, entrypoint: &str) -> Result<Self, String> {
        // Compute PYTHONPATH: the `src` dir is 2 levels up from main.py
        let entrypoint_path = std::path::Path::new(entrypoint);
        let python_path = entrypoint_path
            .parent()    // .../sidecar/
            .and_then(|p| p.parent())  // .../src/
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut child = Command::new(python)
            .arg("-X")
            .arg("utf8")
            .arg("-u")
            .arg(entrypoint)
            .env("PYTHONPATH", &python_path)
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUTF8", "1")
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Failed to spawn sidecar `{python} {entrypoint}` (PYTHONPATH={python_path}): {e}"))?;

        let stdout = child.stdout.take().ok_or("No stdout from sidecar")?;
        let stdin = child.stdin.take().ok_or("No stdin from sidecar")?;

        Ok(Self {
            child,
            reader: BufReader::new(stdout),
            writer: BufWriter::new(stdin),
            next_id: AtomicU64::new(1),
        })
    }

    /// Send a JSON-RPC request and wait for the response.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id.to_string(),
            "method": method,
            "params": params,
        });

        let line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        writeln!(self.writer, "{}", line)
            .map_err(|e| format!("Failed to write to sidecar: {e}"))?;
        self.writer
            .flush()
            .map_err(|e| format!("Failed to flush sidecar stdin: {e}"))?;

        let mut response_line = String::new();
        self.reader
            .read_line(&mut response_line)
            .map_err(|e| format!("Failed to read from sidecar: {e}"))?;

        if response_line.is_empty() {
            return Err("Sidecar returned empty response (process may have crashed)".into());
        }

        let resp: Value = serde_json::from_str(&response_line)
            .map_err(|e| format!("Invalid JSON from sidecar: {e}\nRaw: {response_line}"))?;

        if let Some(error) = resp.get("error") {
            return Err(format!(
                "Sidecar error [{}]: {}",
                error["code"], error["message"]
            ));
        }

        resp.get("result")
            .cloned()
            .ok_or_else(|| "Sidecar returned no result".to_string())
    }

    /// Check if the process is still alive.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Kill the process.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for SidecarProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

// ── Pool ────────────────────────────────────────────────────────
// A small fixed-size pool of sidecar processes. Each `acquire()` call
// hands out one exclusively for the duration of a `SidecarLease`; when
// the lease drops the process returns to the pool. If the held process
// has died (e.g. Python crashed mid-call) the lease drops it and the
// pool re-spawns lazily on next acquire.

pub struct SidecarPool {
    inner: Arc<PoolInner>,
}

struct PoolInner {
    free: Mutex<Vec<SidecarProcess>>,
    cv: Condvar,
    capacity: parking_lot::Mutex<usize>,
    python: String,
    entrypoint: String,
    /// Latest configuration applied to all workers. Updated by `reconfigure()`
    /// and used by `acquire()` when respawning a dead worker.
    cfg: Mutex<SidecarRuntimeCfg>,
}

#[derive(Clone)]
struct SidecarRuntimeCfg {
    api_key: String,
    chat_model: String,
    embedding_model: String,
}

fn configure_payload(cfg: &SidecarRuntimeCfg) -> Value {
    json!({
        "api_key": cfg.api_key,
        "chat_model": cfg.chat_model,
        "embedding_model": cfg.embedding_model,
    })
}

impl SidecarPool {
    pub fn spawn(
        python: &str,
        entrypoint: &str,
        api_key: &str,
        chat_model: &str,
        embedding_model: &str,
        capacity: usize,
    ) -> Result<Self, String> {
        let cap = capacity.max(1);
        let cfg = SidecarRuntimeCfg {
            api_key: api_key.to_string(),
            chat_model: chat_model.to_string(),
            embedding_model: embedding_model.to_string(),
        };
        let payload = configure_payload(&cfg);
        let mut procs = Vec::with_capacity(cap);
        for i in 0..cap {
            let mut p = SidecarProcess::spawn(python, entrypoint).map_err(|e| {
                format!("sidecar pool: failed to spawn worker {} of {}: {e}", i + 1, cap)
            })?;
            p.call("configure", payload.clone())
                .map_err(|e| format!("sidecar pool: configure worker {} failed: {e}", i + 1))?;
            procs.push(p);
        }
        println!(
            "[sidecar] pool ready with {} worker(s) — chat={}, embed={}",
            cap, chat_model, embedding_model
        );
        Ok(Self {
            inner: Arc::new(PoolInner {
                free: Mutex::new(procs),
                cv: Condvar::new(),
                capacity: parking_lot::Mutex::new(cap),
                python: python.to_string(),
                entrypoint: entrypoint.to_string(),
                cfg: Mutex::new(cfg),
            }),
        })
    }

    /// Push new credentials/models to every worker. Drains the pool one slot
    /// at a time so in-flight tasks complete naturally before being touched.
    pub fn reconfigure(
        &self,
        api_key: &str,
        chat_model: &str,
        embedding_model: &str,
    ) -> Result<(), String> {
        let new_cfg = SidecarRuntimeCfg {
            api_key: api_key.to_string(),
            chat_model: chat_model.to_string(),
            embedding_model: embedding_model.to_string(),
        };
        // Update stored cfg first so any respawn during reconfigure uses new values.
        *self.inner.cfg.lock() = new_cfg.clone();
        let payload = configure_payload(&new_cfg);

        // Acquire ALL leases first (blocks if anything is in-flight) so the
        // pool is fully drained — this guarantees every worker is touched
        // exactly once instead of repeatedly grabbing the same just-returned one.
        let cap = self.capacity();
        let mut leases: Vec<SidecarLease> = Vec::with_capacity(cap);
        for _ in 0..cap {
            leases.push(self.acquire());
        }
        for lease in &mut leases {
            lease
                .call("configure", payload.clone())
                .map_err(|e| format!("reconfigure: {e}"))?;
        }
        // All leases drop here — workers return to pool atomically.
        println!(
            "[sidecar] pool reconfigured — chat={}, embed={}",
            chat_model, embedding_model
        );
        Ok(())
    }

    pub fn capacity(&self) -> usize {
        *self.inner.capacity.lock()
    }

    /// Drain every worker, kill it, then spawn a fresh `new_capacity` set.
    /// Blocks until all in-flight tasks complete.
    pub fn resize(&self, new_capacity: usize) -> Result<(), String> {
        let new_cap = new_capacity.max(1);
        let old_cap = self.capacity();
        if new_cap == old_cap {
            return Ok(());
        }

        let mut held: Vec<SidecarLease> = Vec::with_capacity(old_cap);
        for _ in 0..old_cap {
            held.push(self.acquire());
        }
        // Kill the underlying processes by stealing them out of each lease.
        for mut lease in held.drain(..) {
            if let Some(mut p) = lease.proc.take() {
                p.kill();
            }
        }

        let cfg_snapshot = self.inner.cfg.lock().clone();
        let payload = configure_payload(&cfg_snapshot);
        let mut fresh = Vec::with_capacity(new_cap);
        for i in 0..new_cap {
            let mut p = SidecarProcess::spawn(&self.inner.python, &self.inner.entrypoint)
                .map_err(|e| format!("resize: spawn worker {} failed: {e}", i + 1))?;
            p.call("configure", payload.clone())
                .map_err(|e| format!("resize: configure worker {} failed: {e}", i + 1))?;
            fresh.push(p);
        }

        {
            let mut free = self.inner.free.lock();
            *free = fresh;
            *self.inner.capacity.lock() = new_cap;
            self.inner.cv.notify_all();
        }
        println!("[sidecar] pool resized {} → {} workers", old_cap, new_cap);
        Ok(())
    }

    /// Block until a worker is free, then take it. The returned `SidecarLease`
    /// returns the process to the pool when dropped.
    pub fn acquire(&self) -> SidecarLease {
        let mut guard = self.inner.free.lock();
        while guard.is_empty() {
            self.inner.cv.wait(&mut guard);
        }
        let mut proc = guard.pop().unwrap();
        drop(guard);

        // If the worker died since last use, replace it before handing out.
        if !proc.is_alive() {
            eprintln!("[sidecar] pool: detected dead worker, respawning");
            let payload = configure_payload(&self.inner.cfg.lock().clone());
            match SidecarProcess::spawn(&self.inner.python, &self.inner.entrypoint).and_then(|mut p| {
                p.call("configure", payload)?;
                Ok(p)
            }) {
                Ok(new_proc) => proc = new_proc,
                Err(e) => {
                    eprintln!("[sidecar] pool: respawn failed: {e}");
                    // We still return the (dead) one — caller will see the next
                    // call() error and surface it as a task failure.
                }
            }
        }

        SidecarLease {
            inner: Arc::clone(&self.inner),
            proc: Some(proc),
        }
    }
}

impl Clone for SidecarPool {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

pub struct SidecarLease {
    inner: Arc<PoolInner>,
    proc: Option<SidecarProcess>,
}

impl SidecarLease {
    /// Forward a JSON-RPC call to the leased worker.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let p = self
            .proc
            .as_mut()
            .expect("lease used after drop — should be impossible");
        p.call(method, params)
    }
}

impl Drop for SidecarLease {
    fn drop(&mut self) {
        if let Some(proc) = self.proc.take() {
            self.inner.free.lock().push(proc);
            self.inner.cv.notify_one();
        }
    }
}
