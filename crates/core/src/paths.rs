//! Location of the real Clearly Drawn Studios workspace that OpenAccounts
//! supervises. Every job OpenAccounts runs is a script in that directory, so
//! the path is resolved in one place and can be overridden for a move.

use std::{path::PathBuf, process::Command, sync::OnceLock, time::Duration};

pub const DEFAULT_STUDIO_ROOT: &str = r"C:\Users\chrom\Desktop\StickExplainers";

/// The studio workspace root. `OPENACCOUNTS_STUDIO_ROOT` overrides the default.
pub fn studio_root() -> PathBuf {
    std::env::var("OPENACCOUNTS_STUDIO_ROOT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_STUDIO_ROOT))
}

/// The Nova Vexley influencer project (`influencer/` in the OpenAccounts
/// checkout). `OPENACCOUNTS_INFLUENCER_ROOT` overrides; otherwise the folder
/// next to the running app's working directory, else the source checkout.
pub fn influencer_root() -> PathBuf {
    if let Some(explicit) = std::env::var("OPENACCOUNTS_INFLUENCER_ROOT")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        return PathBuf::from(explicit);
    }
    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join("influencer");
        if candidate.join("pipeline.py").is_file() {
            return candidate;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("influencer")
}

/// The interpreter that owns the studio's Python dependencies, resolved once.
///
/// On Windows the `py` launcher maps to the real CPython install; inheriting
/// whatever `python` is first on PATH picks up Inkscape or GIMP and the YouTube
/// client fails. The launcher is only used to *find* the interpreter: jobs are
/// spawned with the resolved `python.exe` directly, because killing `py.exe`
/// on a timeout would leave the real interpreter running. `OPENACCOUNTS_PYTHON`
/// overrides.
pub fn python_interpreter() -> String {
    static RESOLVED: OnceLock<String> = OnceLock::new();
    RESOLVED.get_or_init(resolve_python).clone()
}

fn resolve_python() -> String {
    if let Ok(explicit) = std::env::var("OPENACCOUNTS_PYTHON") {
        if !explicit.trim().is_empty() {
            return explicit.trim().to_string();
        }
    }
    let launcher = if cfg!(windows) { "py" } else { "python3" };
    let mut cmd = Command::new(launcher);
    cmd.args(["-c", "import sys; print(sys.executable)"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(no_window_flag());
    }
    match wait_with_timeout(cmd, Duration::from_secs(10)) {
        Some(path) if !path.is_empty() && std::path::Path::new(&path).is_file() => path,
        _ => launcher.to_string(),
    }
}

/// Runs the command and returns its trimmed stdout, or None on failure or timeout.
fn wait_with_timeout(mut cmd: Command, timeout: Duration) -> Option<String> {
    use std::io::Read;
    cmd.stdout(std::process::Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        stdout.read_to_string(&mut buf).ok().map(|_| buf)
    });
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                return reader.join().ok().flatten().map(|s| s.trim().to_string());
            }
            Ok(Some(_)) => return None,
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
}

/// True on Windows, where child processes must be told not to flash a console.
pub fn no_window_flag() -> u32 {
    #[cfg(windows)]
    {
        0x0800_0000
    }
    #[cfg(not(windows))]
    {
        0
    }
}
