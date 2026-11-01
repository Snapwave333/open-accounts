use openaccounts_core::{OpenAccountsError, Result};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
};
use tracing::{debug, info};

pub const CHROME_PATH_ENV: &str = "OPENACCOUNTS_CHROME_PATH";
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);

fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn search_path(names: &[&str]) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        for name in names {
            let candidate = dir.join(name);
            if is_executable(&candidate) {
                return Some(candidate);
            }
            if cfg!(windows) {
                let exe = dir.join(format!("{name}.exe"));
                if is_executable(&exe) {
                    return Some(exe);
                }
            }
        }
    }
    None
}

fn search_playwright(root: &Path) -> Option<PathBuf> {
    let direct = root.join("chromium");
    if is_executable(&direct) {
        return Some(direct);
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("chromium-"))
                .unwrap_or(false)
        })
        .collect();
    dirs.sort();
    dirs.reverse();
    for dir in dirs {
        for sub in ["chrome-linux64", "chrome-linux", "chrome-win64", "chrome-win", "chrome-mac", "chrome-mac-arm64"] {
            for bin in [
                "chrome",
                "chrome.exe",
                "Chromium.app/Contents/MacOS/Chromium",
                "Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            ] {
                let candidate = dir.join(sub).join(bin);
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

pub fn find_browser_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(CHROME_PATH_ENV).map(PathBuf::from) {
        if is_executable(&p) {
            return Some(p);
        }
    }
    if let Some(p) = search_path(&[
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "chrome",
        "msedge",
    ]) {
        return Some(p);
    }

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(p) = std::env::var_os("PLAYWRIGHT_BROWSERS_PATH") {
        roots.push(PathBuf::from(p));
    }
    roots.push(PathBuf::from("/opt/pw-browsers"));
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".cache/ms-playwright"));
        roots.push(PathBuf::from(home).join("Library/Caches/ms-playwright"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("ms-playwright"));
    }
    for root in roots {
        if let Some(p) = search_playwright(&root) {
            return Some(p);
        }
    }

    let mut fixed: Vec<PathBuf> = vec![
        PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
        PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
        PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
        PathBuf::from("/usr/bin/chromium"),
        PathBuf::from("/usr/bin/google-chrome"),
        PathBuf::from("/snap/bin/chromium"),
    ];
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(var).map(PathBuf::from) {
            fixed.push(base.join(r"Google\Chrome\Application\chrome.exe"));
            fixed.push(base.join(r"Chromium\Application\chrome.exe"));
            fixed.push(base.join(r"Microsoft\Edge\Application\msedge.exe"));
        }
    }
    fixed.into_iter().find(|p| is_executable(p))
}

fn unique_profile_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("openaccounts-chrome-{}-{}", std::process::id(), nanos))
}

pub fn http_base_from_ws(ws_url: &str) -> Option<String> {
    let rest = ws_url.strip_prefix("ws://").or_else(|| ws_url.strip_prefix("wss://"))?;
    let host_port = rest.split('/').next()?;
    if host_port.is_empty() {
        return None;
    }
    Some(format!("http://{host_port}"))
}

pub struct ChromeProcess {
    child: Option<Child>,
    http_base: String,
    browser_ws_url: String,
    user_data_dir: PathBuf,
}

impl ChromeProcess {
    pub async fn launch(binary: Option<PathBuf>) -> Result<Self> {
        let binary = binary.or_else(find_browser_binary).ok_or_else(|| {
            OpenAccountsError::Browser(format!(
                "No Chrome/Chromium binary found; install Chrome or set {CHROME_PATH_ENV}"
            ))
        })?;
        let user_data_dir = unique_profile_dir();
        std::fs::create_dir_all(&user_data_dir)
            .map_err(|e| OpenAccountsError::Browser(format!("Cannot create browser profile dir: {e}")))?;

        let mut child = Command::new(&binary)
            .arg("--headless=new")
            .arg("--no-sandbox")
            .arg("--disable-gpu")
            .arg("--disable-dev-shm-usage")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--remote-debugging-port=0")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--user-data-dir={}", user_data_dir.display()))
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                let _ = std::fs::remove_dir_all(&user_data_dir);
                OpenAccountsError::Browser(format!("Failed to launch {}: {e}", binary.display()))
            })?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| OpenAccountsError::Browser("Browser stderr unavailable".into()))?;
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: Vec<String> = Vec::new();

        let found = tokio::time::timeout(LAUNCH_TIMEOUT, async {
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(idx) = line.find("DevTools listening on ") {
                    return Some(line[idx + "DevTools listening on ".len()..].trim().to_string());
                }
                tail.push(line);
                if tail.len() > 20 {
                    tail.remove(0);
                }
            }
            None
        })
        .await;

        let ws_url = match found {
            Ok(Some(ws)) => ws,
            Ok(None) => {
                let _ = std::fs::remove_dir_all(&user_data_dir);
                return Err(OpenAccountsError::Browser(format!(
                    "Browser exited before exposing DevTools: {}",
                    tail.join(" | ")
                )));
            }
            Err(_) => {
                let _ = child.start_kill();
                let _ = std::fs::remove_dir_all(&user_data_dir);
                return Err(OpenAccountsError::Browser("Timed out waiting for browser DevTools endpoint".into()));
            }
        };

        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                debug!("chrome: {}", line);
            }
        });

        let http_base = http_base_from_ws(&ws_url)
            .ok_or_else(|| OpenAccountsError::Browser(format!("Unrecognised DevTools URL {ws_url}")))?;
        info!("Launched {} with DevTools at {}", binary.display(), http_base);

        Ok(Self {
            child: Some(child),
            http_base,
            browser_ws_url: ws_url,
            user_data_dir,
        })
    }

    pub fn http_base(&self) -> &str {
        &self.http_base
    }

    pub fn browser_ws_url(&self) -> &str {
        &self.browser_ws_url
    }

    pub async fn shutdown(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
        for _ in 0..20 {
            if std::fs::remove_dir_all(&self.user_data_dir).is_ok() || !self.user_data_dir.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Drop for ChromeProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
        let _ = std::fs::remove_dir_all(&self.user_data_dir);
    }
}
