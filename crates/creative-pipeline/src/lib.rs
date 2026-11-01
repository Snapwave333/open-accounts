use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{process::Command, sync::RwLock, task::JoinSet};
use tracing::{info, warn};
use uuid::Uuid;

pub const ARTIFACTS_DIR_ENV: &str = "OPENACCOUNTS_ARTIFACTS_DIR";
pub const FONT_FILE_ENV: &str = "OPENACCOUNTS_FONT_FILE";
pub const TARGET_LUFS: f32 = -14.0;
pub const SILENCE_LUFS: f32 = -70.0;
pub const DEFAULT_BACKGROUND_HEX: &str = "#141722";
pub const DEFAULT_VIDEO_SECONDS: f32 = 15.0;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const RENDER_TIMEOUT: Duration = Duration::from_secs(900);
const ANALYSIS_TIMEOUT: Duration = Duration::from_secs(300);
const RATIO_TOLERANCE: f64 = 0.03;
const LUFS_WARNING_DELTA: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaAspectRatio {
    Vertical9x16,
    Square1x1,
    Landscape16x9,
}

impl MediaAspectRatio {
    pub fn dimensions(&self) -> (u32, u32) {
        match self {
            MediaAspectRatio::Vertical9x16 => (1080, 1920),
            MediaAspectRatio::Square1x1 => (1080, 1080),
            MediaAspectRatio::Landscape16x9 => (1920, 1080),
        }
    }

    pub fn ratio(&self) -> f64 {
        let (w, h) = self.dimensions();
        w as f64 / h as f64
    }

    pub fn classify(width: u32, height: u32) -> Option<MediaAspectRatio> {
        if width == 0 || height == 0 {
            return None;
        }
        let actual = width as f64 / height as f64;
        [
            MediaAspectRatio::Vertical9x16,
            MediaAspectRatio::Square1x1,
            MediaAspectRatio::Landscape16x9,
        ]
        .into_iter()
        .find(|r| ((actual - r.ratio()) / r.ratio()).abs() <= RATIO_TOLERANCE)
    }

    pub fn nearest(width: u32, height: u32) -> MediaAspectRatio {
        if height == 0 {
            return MediaAspectRatio::Landscape16x9;
        }
        let actual = width as f64 / height as f64;
        [
            MediaAspectRatio::Vertical9x16,
            MediaAspectRatio::Square1x1,
            MediaAspectRatio::Landscape16x9,
        ]
        .into_iter()
        .min_by(|a, b| (actual - a.ratio()).abs().total_cmp(&(actual - b.ratio()).abs()))
        .unwrap_or(MediaAspectRatio::Landscape16x9)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapability {
    pub name: String,
    pub command: String,
    pub is_available: bool,
    pub version: Option<String>,
    pub supported_features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolVerificationResult {
    pub tool_name: String,
    pub command: String,
    pub is_installed: bool,
    pub version: Option<String>,
    pub output_sample: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaValidationResult {
    pub is_valid: bool,
    pub aspect_ratio: MediaAspectRatio,
    pub duration_seconds: f32,
    pub audio_lufs: f32,
    pub has_audio: bool,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeAssetSpec {
    pub asset_id: String,
    pub title: String,
    pub output_format: String,
    pub target_ratio: MediaAspectRatio,
    pub target_platforms: Vec<String>,
    pub script_or_prompt: String,
    pub brand_id: String,
    pub tools_required: Vec<String>,
    #[serde(default)]
    pub background_hex: Option<String>,
    #[serde(default)]
    pub duration_seconds: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeJobStatus {
    pub job_id: String,
    pub asset_id: String,
    pub progress_percent: u8,
    pub current_stage: String,
    pub output_path: Option<String>,
    pub validation: Option<MediaValidationResult>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

pub const JOB_QUEUED: &str = "Queued";
pub const JOB_PROCESSING: &str = "Processing";
pub const JOB_COMPLETED: &str = "Completed";
pub const JOB_FAILED: &str = "Failed";

struct ToolDefinition {
    id: &'static str,
    name: &'static str,
    command: &'static str,
    version_args: &'static [&'static str],
    features: &'static [&'static str],
}

const TOOL_DEFINITIONS: &[ToolDefinition] = &[
    ToolDefinition {
        id: "ffmpeg",
        name: "FFmpeg CLI",
        command: "ffmpeg",
        version_args: &["-version"],
        features: &["Video Transcode", "Audio Normalization", "Subtitles Burn-in", "Reel Resizing (9:16)"],
    },
    ToolDefinition {
        id: "ffprobe",
        name: "FFprobe Media Inspector",
        command: "ffprobe",
        version_args: &["-version"],
        features: &["Stream Inspection", "Duration & Resolution Validation"],
    },
    ToolDefinition {
        id: "blender",
        name: "Blender 3D",
        command: "blender",
        version_args: &["--version"],
        features: &["3D Modeling", "Cinematic Camera", "Eevee/Cycles Render"],
    },
    ToolDefinition {
        id: "vectorcraft",
        name: "Vector Craft CLI",
        command: "vectorcraft-cli",
        version_args: &["--version"],
        features: &["Vector Layouts", "SVG/PDF Render"],
    },
    ToolDefinition {
        id: "photocraft",
        name: "Photo Craft CLI",
        command: "photocraft-cli",
        version_args: &["--version"],
        features: &["Image Editing", "Compositing", "Batch Grade"],
    },
    ToolDefinition {
        id: "effectcraft",
        name: "Effect Craft CLI",
        command: "effectcraft-cli",
        version_args: &["--version"],
        features: &["Motion Graphics", "Lottie Export", "MP4 Render"],
    },
    ToolDefinition {
        id: "opentoonz",
        name: "OpenToonz Animation Suite",
        command: "tcomposer",
        version_args: &["-help"],
        features: &["2D Animation", "Scene Assembly"],
    },
    ToolDefinition {
        id: "imagemagick",
        name: "ImageMagick",
        command: "magick",
        version_args: &["-version"],
        features: &["Image Conversion", "Thumbnail Generation", "Compositing"],
    },
];

fn version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d+\.\d+(?:\.\d+)?(?:[-.][0-9A-Za-z]+)?").expect("valid version regex"))
}

fn hex_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^#?([0-9a-fA-F]{6})$").expect("valid hex regex"))
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub async fn run_command(program: &str, args: &[String], timeout: Duration) -> Result<CommandOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = cmd
        .spawn()
        .map_err(|e| OpenAccountsError::Internal(format!("Unable to launch '{program}': {e}")))?;
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(out)) => Ok(CommandOutput {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }),
        Ok(Err(e)) => Err(OpenAccountsError::Internal(format!("'{program}' failed: {e}"))),
        Err(_) => Err(OpenAccountsError::Internal(format!(
            "'{program}' timed out after {}s",
            timeout.as_secs()
        ))),
    }
}

pub fn parse_tool_version(output: &str) -> Option<String> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(5)
        .find_map(|l| version_re().find(l).map(|m| m.as_str().to_string()))
}

async fn probe_definition(def: &ToolDefinition) -> ToolVerificationResult {
    let args: Vec<String> = def.version_args.iter().map(|s| s.to_string()).collect();
    let command_line = format!("{} {}", def.command, def.version_args.join(" "));
    match run_command(def.command, &args, PROBE_TIMEOUT).await {
        Ok(out) => {
            let combined = format!("{}\n{}", out.stdout, out.stderr);
            let produced_output = !combined.trim().is_empty();
            let installed = out.success || produced_output;
            let sample = combined
                .lines()
                .find(|l| !l.trim().is_empty())
                .map(|l| l.trim().chars().take(200).collect::<String>());
            ToolVerificationResult {
                tool_name: def.name.into(),
                command: command_line,
                is_installed: installed,
                version: if installed { parse_tool_version(&combined) } else { None },
                output_sample: sample,
                error_message: if installed {
                    None
                } else {
                    Some(format!("'{}' exited unsuccessfully without output", def.command))
                },
            }
        }
        Err(e) => ToolVerificationResult {
            tool_name: def.name.into(),
            command: command_line,
            is_installed: false,
            version: None,
            output_sample: None,
            error_message: Some(e.to_string()),
        },
    }
}

pub fn artifacts_dir() -> PathBuf {
    std::env::var(ARTIFACTS_DIR_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("openaccounts").join("renders"))
}

pub fn normalize_hex_color(input: Option<&str>) -> Result<String> {
    let raw = input.unwrap_or(DEFAULT_BACKGROUND_HEX).trim();
    hex_re()
        .captures(raw)
        .map(|c| format!("0x{}", c[1].to_uppercase()))
        .ok_or_else(|| OpenAccountsError::Internal(format!("Invalid background colour '{raw}', expected #RRGGBB")))
}

pub fn wrap_text(text: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(8);
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let word_len = word.chars().count();
            if word_len > max_chars {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                let chars: Vec<char> = word.chars().collect();
                for chunk in chars.chunks(max_chars) {
                    lines.push(chunk.iter().collect());
                }
                continue;
            }
            let needed = if current.is_empty() { word_len } else { current.chars().count() + 1 + word_len };
            if needed > max_chars && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines.join("\n")
}

pub fn escape_filter_path(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let mut out = String::with_capacity(raw.len() + 8);
    for c in raw.chars() {
        if matches!(c, ':' | '\'' | '\\' | ',' | ';' | '[' | ']' | '=') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn find_font_file() -> Option<PathBuf> {
    if let Some(p) = std::env::var(FONT_FILE_ENV).ok().filter(|v| !v.trim().is_empty()) {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/Library/Fonts/Arial.ttf",
        "C:\\Windows\\Fonts\\arialbd.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

pub fn platform_max_duration_seconds(platform: &str, ratio: MediaAspectRatio) -> Option<f32> {
    match platform.to_lowercase().as_str() {
        "instagram" => Some(180.0),
        "tiktok" => Some(600.0),
        "youtube" if ratio == MediaAspectRatio::Vertical9x16 => Some(180.0),
        "youtube" => Some(43_200.0),
        "x" | "twitter" => Some(140.0),
        "threads" => Some(300.0),
        "facebook" => Some(14_400.0),
        "linkedin" => Some(900.0),
        "reddit" => Some(900.0),
        _ => None,
    }
}

pub fn parse_loudnorm_input_i(stderr: &str) -> Option<f32> {
    let start = stderr.rfind('{')?;
    let end = stderr[start..].find('}')? + start;
    let json: serde_json::Value = serde_json::from_str(&stderr[start..=end]).ok()?;
    let raw = json["input_i"].as_str()?;
    if raw.contains("inf") {
        return Some(SILENCE_LUFS);
    }
    raw.trim().parse::<f32>().ok().map(|v| v.max(SILENCE_LUFS))
}

#[derive(Debug, Clone, Default)]
pub struct ProbeSummary {
    pub width: u32,
    pub height: u32,
    pub duration_seconds: f32,
    pub has_audio: bool,
    pub has_video: bool,
}

pub fn parse_ffprobe_json(raw: &str) -> Result<ProbeSummary> {
    let json: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| OpenAccountsError::Internal(format!("Unreadable ffprobe output: {e}")))?;
    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let video = streams.iter().find(|s| s["codec_type"] == "video");
    let has_audio = streams.iter().any(|s| s["codec_type"] == "audio");
    let parse_f32 = |v: &serde_json::Value| v.as_str().and_then(|s| s.parse::<f32>().ok());
    let duration = parse_f32(&json["format"]["duration"])
        .or_else(|| video.and_then(|v| parse_f32(&v["duration"])))
        .unwrap_or(0.0);
    let (mut width, mut height) = video
        .map(|v| (v["width"].as_u64().unwrap_or(0) as u32, v["height"].as_u64().unwrap_or(0) as u32))
        .unwrap_or((0, 0));
    let rotation = video
        .and_then(|v| v["side_data_list"].as_array())
        .and_then(|list| list.iter().find_map(|d| d["rotation"].as_i64()))
        .or_else(|| video.and_then(|v| v["tags"]["rotate"].as_str()).and_then(|r| r.parse::<i64>().ok()))
        .unwrap_or(0);
    if rotation.abs() % 180 == 90 {
        std::mem::swap(&mut width, &mut height);
    }
    Ok(ProbeSummary {
        width,
        height,
        duration_seconds: duration,
        has_audio,
        has_video: video.is_some(),
    })
}

struct JobRecord {
    status: CreativeJobStatus,
    spec: CreativeAssetSpec,
}

pub struct CreativePipeline {
    tools: Arc<RwLock<HashMap<String, ToolCapability>>>,
    jobs: Arc<RwLock<HashMap<String, JobRecord>>>,
}

impl CreativePipeline {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn discover_tools(&self) {
        info!("Probing host for creative production tools...");
        let mut set = JoinSet::new();
        for (idx, def) in TOOL_DEFINITIONS.iter().enumerate() {
            set.spawn(async move { (idx, probe_definition(def).await) });
        }

        let mut tool_map = HashMap::new();
        while let Some(joined) = set.join_next().await {
            let Ok((idx, result)) = joined else { continue };
            let def = &TOOL_DEFINITIONS[idx];
            tool_map.insert(
                def.id.to_string(),
                ToolCapability {
                    name: def.name.into(),
                    command: def.command.into(),
                    is_available: result.is_installed,
                    version: result.version,
                    supported_features: def.features.iter().map(|f| f.to_string()).collect(),
                },
            );
        }

        let available = tool_map.values().filter(|t| t.is_available).count();
        info!("Creative tool discovery: {}/{} tools available", available, tool_map.len());
        *self.tools.write().await = tool_map;
    }

    pub async fn list_tools(&self) -> Vec<ToolCapability> {
        let mut tools: Vec<ToolCapability> = self.tools.read().await.values().cloned().collect();
        tools.sort_by(|a, b| b.is_available.cmp(&a.is_available).then_with(|| a.name.cmp(&b.name)));
        tools
    }

    pub async fn verify_cli_tool(&self, tool_id: &str) -> ToolVerificationResult {
        match TOOL_DEFINITIONS.iter().find(|d| d.id == tool_id) {
            Some(def) => {
                let result = probe_definition(def).await;
                if let Some(tool) = self.tools.write().await.get_mut(tool_id) {
                    tool.is_available = result.is_installed;
                    tool.version = result.version.clone();
                }
                result
            }
            None => ToolVerificationResult {
                tool_name: tool_id.to_string(),
                command: tool_id.to_string(),
                is_installed: false,
                version: None,
                output_sample: None,
                error_message: Some(format!("Unknown tool '{tool_id}'")),
            },
        }
    }

    pub async fn create_production_job(&self, spec: CreativeAssetSpec) -> Result<String> {
        if spec.asset_id.trim().is_empty() {
            return Err(OpenAccountsError::Internal("Creative asset id cannot be empty".into()));
        }
        normalize_hex_color(spec.background_hex.as_deref())?;
        let job_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let status = CreativeJobStatus {
            job_id: job_id.clone(),
            asset_id: spec.asset_id.clone(),
            progress_percent: 0,
            current_stage: "Queued for rendering".into(),
            output_path: None,
            validation: None,
            status: JOB_QUEUED.into(),
            created_at: now,
            updated_at: Some(now),
            error_message: None,
            warnings: vec![],
        };
        info!("Queued creative job {} for asset {}", job_id, spec.asset_id);
        self.jobs.write().await.insert(job_id.clone(), JobRecord { status, spec });
        Ok(job_id)
    }

    async fn update_job<F: FnOnce(&mut CreativeJobStatus)>(&self, job_id: &str, f: F) -> Option<CreativeJobStatus> {
        let mut jobs = self.jobs.write().await;
        let record = jobs.get_mut(job_id)?;
        f(&mut record.status);
        record.status.updated_at = Some(Utc::now());
        Some(record.status.clone())
    }

    pub async fn execute_job(&self, job_id: &str) -> Result<CreativeJobStatus> {
        let spec = {
            let mut jobs = self.jobs.write().await;
            let record = jobs
                .get_mut(job_id)
                .ok_or_else(|| OpenAccountsError::Internal("Creative job not found".into()))?;
            match record.status.status.as_str() {
                JOB_PROCESSING => {
                    return Err(OpenAccountsError::Internal(format!("Creative job {job_id} is already rendering")))
                }
                JOB_COMPLETED => return Ok(record.status.clone()),
                _ => {}
            }
            record.status.status = JOB_PROCESSING.into();
            record.status.progress_percent = 5;
            record.status.current_stage = "Preparing render".into();
            record.status.error_message = None;
            record.status.warnings.clear();
            record.status.updated_at = Some(Utc::now());
            record.spec.clone()
        };

        match self.render(job_id, &spec).await {
            Ok((output, validation, warnings)) => {
                let valid = validation.is_valid;
                let errors = validation.errors.join("; ");
                let status = self
                    .update_job(job_id, |s| {
                        s.output_path = Some(output.to_string_lossy().into_owned());
                        s.warnings = warnings;
                        s.warnings.extend(validation.warnings.iter().cloned());
                        s.validation = Some(validation);
                        s.progress_percent = 100;
                        if valid {
                            s.status = JOB_COMPLETED.into();
                            s.current_stage = "Render complete and validated".into();
                        } else {
                            s.status = JOB_FAILED.into();
                            s.current_stage = "Render failed validation".into();
                            s.error_message = Some(errors);
                        }
                    })
                    .await
                    .ok_or_else(|| OpenAccountsError::Internal("Creative job disappeared".into()))?;
                info!("Creative job {} finished with status {}", job_id, status.status);
                Ok(status)
            }
            Err(e) => {
                warn!("Creative job {} failed: {}", job_id, e);
                let message = e.to_string();
                self.update_job(job_id, |s| {
                    s.status = JOB_FAILED.into();
                    s.current_stage = "Render failed".into();
                    s.error_message = Some(message);
                })
                .await
                .ok_or_else(|| OpenAccountsError::Internal("Creative job disappeared".into()))
            }
        }
    }

    async fn render(
        &self,
        job_id: &str,
        spec: &CreativeAssetSpec,
    ) -> Result<(PathBuf, MediaValidationResult, Vec<String>)> {
        let format = spec.output_format.trim().to_lowercase();
        let is_still = match format.as_str() {
            "mp4" => false,
            "png" => true,
            other => {
                return Err(OpenAccountsError::Internal(format!(
                    "Unsupported output format '{other}' (supported: mp4, png)"
                )))
            }
        };

        let ffmpeg = self.verify_cli_tool("ffmpeg").await;
        if !ffmpeg.is_installed {
            return Err(OpenAccountsError::Internal(format!(
                "FFmpeg is required for rendering but is not available: {}",
                ffmpeg.error_message.unwrap_or_default()
            )));
        }

        let job_dir = artifacts_dir().join(job_id);
        tokio::fs::create_dir_all(&job_dir)
            .await
            .map_err(|e| OpenAccountsError::Internal(format!("Cannot create artifacts dir {}: {e}", job_dir.display())))?;

        let safe_asset: String = spec
            .asset_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .take(64)
            .collect();
        let output = job_dir.join(format!("{safe_asset}.{format}"));

        let (width, height) = spec.target_ratio.dimensions();
        let color = normalize_hex_color(spec.background_hex.as_deref())?;
        let duration = spec.duration_seconds.unwrap_or(DEFAULT_VIDEO_SECONDS).clamp(1.0, 600.0);
        let mut warnings = Vec::new();

        self.update_job(job_id, |s| {
            s.progress_percent = 15;
            s.current_stage = "Composing text layers".into();
        })
        .await;

        let filters = run_command("ffmpeg", &["-hide_banner".into(), "-filters".into()], PROBE_TIMEOUT).await?;
        let has_drawtext = filters.stdout.lines().any(|l| l.split_whitespace().nth(1) == Some("drawtext"));
        let font = find_font_file();

        let mut video_filters: Vec<String> = Vec::new();
        let has_text = !spec.title.trim().is_empty() || !spec.script_or_prompt.trim().is_empty();
        if has_text && has_drawtext {
            let font_arg = match &font {
                Some(f) => format!("fontfile='{}':", escape_filter_path(f)),
                None => {
                    warnings.push("No font file found; relying on fontconfig default font".into());
                    String::new()
                }
            };
            let title_size = (width / 14).max(24);
            let body_size = (width / 26).max(18);
            let chars_for = |size: u32| ((width as f32 * 0.86) / (size as f32 * 0.56)) as usize;

            if !spec.title.trim().is_empty() {
                let path = job_dir.join("title.txt");
                tokio::fs::write(&path, wrap_text(spec.title.trim(), chars_for(title_size)))
                    .await
                    .map_err(|e| OpenAccountsError::Internal(format!("Cannot write title text: {e}")))?;
                video_filters.push(format!(
                    "drawtext={font_arg}textfile='{}':fontcolor=white:fontsize={title_size}:line_spacing={}:x=(w-text_w)/2:y=h*0.16",
                    escape_filter_path(&path),
                    title_size / 4
                ));
            }
            if !spec.script_or_prompt.trim().is_empty() {
                let path = job_dir.join("script.txt");
                tokio::fs::write(&path, wrap_text(spec.script_or_prompt.trim(), chars_for(body_size)))
                    .await
                    .map_err(|e| OpenAccountsError::Internal(format!("Cannot write script text: {e}")))?;
                video_filters.push(format!(
                    "drawtext={font_arg}textfile='{}':fontcolor=white@0.92:fontsize={body_size}:line_spacing={}:x=(w-text_w)/2:y=h*0.38",
                    escape_filter_path(&path),
                    body_size / 3
                ));
            }
        } else if has_text {
            warnings.push("FFmpeg build lacks the drawtext filter; rendered without text overlays".into());
        }
        video_filters.push("format=yuv420p".into());

        let mut args: Vec<String> = vec![
            "-y".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-f".into(),
            "lavfi".into(),
            "-i".into(),
            format!("color=c={color}:s={width}x{height}:r=30:d={duration:.3}"),
        ];
        if is_still {
            args.extend(["-vf".into(), video_filters.join(","), "-frames:v".into(), "1".into()]);
        } else {
            args.extend([
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                "anullsrc=channel_layout=stereo:sample_rate=48000".into(),
                "-t".into(),
                format!("{duration:.3}"),
                "-vf".into(),
                video_filters.join(","),
                "-af".into(),
                "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo".into(),
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "veryfast".into(),
                "-crf".into(),
                "20".into(),
                "-pix_fmt".into(),
                "yuv420p".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "128k".into(),
                "-shortest".into(),
                "-movflags".into(),
                "+faststart".into(),
            ]);
        }
        args.push(output.to_string_lossy().into_owned());

        self.update_job(job_id, |s| {
            s.progress_percent = 35;
            s.current_stage = format!("Encoding {width}x{height} with FFmpeg");
        })
        .await;

        let rendered = run_command("ffmpeg", &args, RENDER_TIMEOUT).await?;
        if !rendered.success || !output.is_file() {
            let detail: String = rendered.stderr.trim().chars().take(600).collect();
            return Err(OpenAccountsError::Internal(format!("FFmpeg render failed: {detail}")));
        }

        self.update_job(job_id, |s| {
            s.progress_percent = 80;
            s.current_stage = "Validating output media".into();
        })
        .await;

        let mut validation = self
            .validate_media_for(&output.to_string_lossy(), spec.target_ratio, &spec.target_platforms)
            .await?;
        if is_still {
            validation.errors.retain(|e| !e.contains("audio"));
            validation.warnings.retain(|w| !w.to_lowercase().contains("audio"));
            validation.is_valid = validation.errors.is_empty();
        }
        Ok((output, validation, warnings))
    }

    pub async fn get_job_status(&self, job_id: &str) -> Option<CreativeJobStatus> {
        self.jobs.read().await.get(job_id).map(|r| r.status.clone())
    }

    pub async fn get_job_spec(&self, job_id: &str) -> Option<CreativeAssetSpec> {
        self.jobs.read().await.get(job_id).map(|r| r.spec.clone())
    }

    pub async fn list_jobs(&self) -> Vec<CreativeJobStatus> {
        let mut jobs: Vec<CreativeJobStatus> = self.jobs.read().await.values().map(|r| r.status.clone()).collect();
        jobs.sort_by_key(|j| std::cmp::Reverse(j.created_at));
        jobs
    }

    pub async fn validate_media(&self, file_path: &str, expected_ratio: MediaAspectRatio) -> Result<MediaValidationResult> {
        self.validate_media_for(file_path, expected_ratio, &[]).await
    }

    pub async fn validate_media_for(
        &self,
        file_path: &str,
        expected_ratio: MediaAspectRatio,
        target_platforms: &[String],
    ) -> Result<MediaValidationResult> {
        if !Path::new(file_path).is_file() {
            return Ok(MediaValidationResult {
                is_valid: false,
                aspect_ratio: expected_ratio,
                duration_seconds: 0.0,
                audio_lufs: SILENCE_LUFS,
                has_audio: false,
                width: 0,
                height: 0,
                errors: vec![format!("Media file not found: {file_path}")],
                warnings: vec![],
            });
        }

        let probe = run_command(
            "ffprobe",
            &[
                "-v".into(),
                "error".into(),
                "-print_format".into(),
                "json".into(),
                "-show_streams".into(),
                "-show_format".into(),
                file_path.into(),
            ],
            ANALYSIS_TIMEOUT,
        )
        .await?;
        if !probe.success {
            return Err(OpenAccountsError::Internal(format!(
                "ffprobe could not read {file_path}: {}",
                probe.stderr.trim()
            )));
        }
        let summary = parse_ffprobe_json(&probe.stdout)?;

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if !summary.has_video {
            errors.push("No video stream found".into());
        }
        let detected = MediaAspectRatio::classify(summary.width, summary.height);
        let aspect_ratio = detected.unwrap_or_else(|| MediaAspectRatio::nearest(summary.width, summary.height));
        match detected {
            Some(r) if r == expected_ratio => {}
            Some(r) => errors.push(format!(
                "Aspect ratio {:?} ({}x{}) does not match expected {:?}",
                r, summary.width, summary.height, expected_ratio
            )),
            None => errors.push(format!(
                "Non-standard aspect ratio {}x{} (expected {:?})",
                summary.width, summary.height, expected_ratio
            )),
        }

        for platform in target_platforms {
            if let Some(max) = platform_max_duration_seconds(platform, expected_ratio) {
                if summary.duration_seconds > max {
                    errors.push(format!(
                        "Duration {:.1}s exceeds the {}s maximum for {}",
                        summary.duration_seconds, max, platform
                    ));
                }
            }
        }

        let audio_lufs = if summary.has_audio {
            let analysis = run_command(
                "ffmpeg",
                &[
                    "-hide_banner".into(),
                    "-nostats".into(),
                    "-i".into(),
                    file_path.into(),
                    "-af".into(),
                    "loudnorm=print_format=json".into(),
                    "-f".into(),
                    "null".into(),
                    "-".into(),
                ],
                ANALYSIS_TIMEOUT,
            )
            .await?;
            match parse_loudnorm_input_i(&analysis.stderr) {
                Some(v) => v,
                None => {
                    warnings.push("Could not measure integrated loudness".into());
                    SILENCE_LUFS
                }
            }
        } else {
            warnings.push("Media has no audio track".into());
            SILENCE_LUFS
        };

        if summary.has_audio {
            if audio_lufs <= SILENCE_LUFS {
                warnings.push("Audio track is silent".into());
            } else if (audio_lufs - TARGET_LUFS).abs() > LUFS_WARNING_DELTA {
                warnings.push(format!(
                    "Integrated loudness {:.1} LUFS is far from the {:.0} LUFS streaming target",
                    audio_lufs, TARGET_LUFS
                ));
            }
        }

        Ok(MediaValidationResult {
            is_valid: errors.is_empty(),
            aspect_ratio,
            duration_seconds: summary.duration_seconds,
            audio_lufs,
            has_audio: summary.has_audio,
            width: summary.width,
            height: summary.height,
            errors,
            warnings,
        })
    }
}

impl Default for CreativePipeline {
    fn default() -> Self {
        Self::new()
    }
}
