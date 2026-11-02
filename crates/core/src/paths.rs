//! Location of the real Clearly Drawn Studios workspace that OpenAccounts
//! supervises. Every job OpenAccounts runs is a script in that directory, so
//! the path is resolved in one place and can be overridden for a move.

use std::path::PathBuf;

pub const DEFAULT_STUDIO_ROOT: &str = r"C:\Users\chrom\Desktop\StickExplainers";

/// The studio workspace root. `OPENACCOUNTS_STUDIO_ROOT` overrides the default.
pub fn studio_root() -> PathBuf {
    std::env::var("OPENACCOUNTS_STUDIO_ROOT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_STUDIO_ROOT))
}

/// The interpreter that owns the studio's Python dependencies. On Windows the
/// `py` launcher maps to the real CPython install; inheriting whatever `python`
/// is first on PATH picks up Inkscape or GIMP and the YouTube client fails.
/// `OPENACCOUNTS_PYTHON` overrides.
pub fn python_interpreter() -> String {
    if let Ok(explicit) = std::env::var("OPENACCOUNTS_PYTHON") {
        if !explicit.trim().is_empty() {
            return explicit.trim().to_string();
        }
    }
    #[cfg(windows)]
    {
        return "py".to_string();
    }
    #[allow(unreachable_code)]
    "python3".to_string()
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
