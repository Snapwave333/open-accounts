use chrono::{DateTime, Utc};
use openaccounts_core::{OpenAccountsError, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, process::Stdio, sync::Arc, time::Duration};
use tokio::{process::Command, sync::RwLock};
use tracing::info;

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const FFPROBE_TIMEOUT: Duration = Duration::from_secs(60);
const LOUDNESS_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaAspectRatio {
    Vertical9x16,  // Instagram Reels, TikTok, YouTube Shorts (1080x1920)
    Square1x1,     // Instagram Feed, Carousel (1080x1080)
    Landscape16x9, // YouTube Standard, X Video (1920x1080)
}

impl MediaAspectRatio {
    fn ratio(self) -> f64 {
        match self {
            MediaAspectRatio::Vertical9x16 => 9.0 / 16.0,
            MediaAspectRatio::Square1x1 => 1.0,
            MediaAspectRatio::Landscape16x9 => 16.0 / 9.0,
        }
    }

    /// The ratio a frame of `width` x `height` matches, if any (2% tolerance).
    pub fn classify(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let actual = width as f64 / height as f64;
        [Self::Vertical9x16, Self::Square1x1, Self::Landscape16x9]
            .into_iter()
            .find(|r| ((actual - r.ratio()) / r.ratio()).abs() <= 0.02)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapability {
    pub name: String,
    pub command: String,
    pub is_available: bool,
    /// First line the tool printed when asked for its version.
    pub version: Option<String>,
    pub supported_features: Vec<String>,
    /// Why the probe failed, when it did.
    pub probe_error: Option<String>,
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
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The ratio the file actually has, if it matches a supported one.
    pub aspect_ratio: Option<MediaAspectRatio>,
    pub duration_seconds: Option<f32>,
    /// Integrated loudness measured by ffmpeg's ebur128 filter.
    pub audio_lufs: Option<f32>,
    pub has_audio: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeAssetSpec {
    pub asset_id: String,
    pub title: String,
    pub output_format: String, // "mp4", "png", "svg", "webm"
    pub target_ratio: MediaAspectRatio,
    pub target_platforms: Vec<String>,
    pub script_or_prompt: String,
    pub brand_id: String,
    pub tools_required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeJobStatus {
    pub job_id: String,
    pub asset_id: String,
    pub progress_percent: u8,
    pub current_stage: String,
    pub output_path: Option<String>,
    pub validation: Option<MediaValidationResult>,
    pub status: String, // "Queued", "Processing", "Completed", "Failed"
    pub created_at: DateTime<Utc>,
}

/// A creative CLI the pipeline knows how to probe.
struct ToolSpec {
    id: &'static str,
    name: &'static str,
    command: &'static str,
    version_args: &'static [&'static str],
    features: &'static [&'static str],
}

const TOOL_SPECS: &[ToolSpec] = &[
    ToolSpec {
        id: "ffmpeg",
        name: "FFmpeg CLI",
        command: "ffmpeg",
        version_args: &["-version"],
        features: &["Video Transcode", "Audio Normalization", "Subtitles Burn-in", "Reel Resizing (9:16)"],
    },
    ToolSpec {
        id: "ffprobe",
        name: "FFprobe",
        command: "ffprobe",
        version_args: &["-version"],
        features: &["Media Inspection", "Output Validation"],
    },
    ToolSpec {
        id: "blender",
        name: "Blender 3D",
        command: "blender",
        version_args: &["--version"],
        features: &["3D Modeling", "Cinematic Camera", "Eevee/Cycles Render"],
    },
    ToolSpec {
        id: "opentoonz",
        name: "OpenToonz / tcomposer",
        command: "tcomposer",
        version_args: &["-help"],
        features: &["2D Animation", "Scene Assembly", "Cleanup"],
    },
    ToolSpec {
        id: "vectorcraft",
        name: "Vector Craft CLI",
        command: "vectorcraft-cli",
        version_args: &["--version"],
        features: &["Vector Layouts", "SVG/PDF Render"],
    },
    ToolSpec {
        id: "photocraft",
        name: "Photo Craft CLI",
        command: "photocraft-cli",
        version_args: &["--version"],
        features: &["Image Editing", "Compositing"],
    },
    ToolSpec {
        id: "effectcraft",
        name: "Effect Craft CLI",
        command: "effectcraft-cli",
        version_args: &["--version"],
        features: &["Motion Graphics", "Compositing"],
    },
];

struct ProcessOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

async fn run_tool(program: &str, args: &[&str], limit: Duration) -> Result<ProcessOutput> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(openaccounts_core::no_window_flag());

    let output = tokio::time::timeout(limit, command.output())
        .await
        .map_err(|_| {
            OpenAccountsError::Automation(format!("{program} did not finish within {}s", limit.as_secs()))
        })?
        .map_err(|e| OpenAccountsError::NotConfigured(format!("{program} could not be started: {e}")))?;

    Ok(ProcessOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(200).collect())
}

async fn probe(spec: &ToolSpec) -> ToolVerificationResult {
    let command_line = format!("{} {}", spec.command, spec.version_args.join(" "));
    match run_tool(spec.command, spec.version_args, PROBE_TIMEOUT).await {
        Ok(out) => {
            let line = first_line(&out.stdout).or_else(|| first_line(&out.stderr));
            // Some tools exit non-zero for their help text; output proves the binary runs.
            let installed = out.success || line.is_some();
            ToolVerificationResult {
                tool_name: spec.name.into(),
                command: command_line,
                is_installed: installed,
                version: line.clone(),
                output_sample: line,
                error_message: if installed {
                    None
                } else {
                    Some(format!("{} exited with an error and printed nothing", spec.command))
                },
            }
        }
        Err(e) => ToolVerificationResult {
            tool_name: spec.name.into(),
            command: command_line,
            is_installed: false,
            version: None,
            output_sample: None,
            error_message: Some(e.to_string()),
        },
    }
}

pub struct CreativePipeline {
    tools: Arc<RwLock<HashMap<String, ToolCapability>>>,
    jobs: Arc<RwLock<HashMap<String, CreativeJobStatus>>>,
}

impl CreativePipeline {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Probes every known creative CLI on this machine and records what is
    /// actually installed.
    pub async fn discover_tools(&self) {
        info!("Probing creative CLIs on this host...");
        // Probe concurrently; each spec is 'static, so the tasks can be spawned.
        let handles: Vec<_> = TOOL_SPECS
            .iter()
            .map(|spec| (spec, tokio::spawn(probe(spec))))
            .collect();

        let mut tool_map = HashMap::new();
        for (spec, handle) in handles {
            let result = match handle.await {
                Ok(result) => result,
                Err(e) => ToolVerificationResult {
                    tool_name: spec.name.into(),
                    command: spec.command.into(),
                    is_installed: false,
                    version: None,
                    output_sample: None,
                    error_message: Some(format!("Probe task failed: {e}")),
                },
            };
            tool_map.insert(
                spec.id.to_string(),
                ToolCapability {
                    name: spec.name.into(),
                    command: spec.command.into(),
                    is_available: result.is_installed,
                    version: result.version,
                    supported_features: spec.features.iter().map(|f| f.to_string()).collect(),
                    probe_error: result.error_message,
                },
            );
        }

        let available = tool_map.values().filter(|t| t.is_available).count();
        info!("Creative tool probe: {available} of {} available", tool_map.len());
        *self.tools.write().await = tool_map;
    }

    pub async fn list_tools(&self) -> Vec<ToolCapability> {
        let mut tools: Vec<ToolCapability> = self.tools.read().await.values().cloned().collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        tools
    }

    /// Runs the tool's version command now and reports the real result.
    pub async fn verify_cli_tool(&self, tool_id: &str) -> ToolVerificationResult {
        match TOOL_SPECS.iter().find(|s| s.id == tool_id) {
            Some(spec) => probe(spec).await,
            None => ToolVerificationResult {
                tool_name: tool_id.to_string(),
                command: tool_id.to_string(),
                is_installed: false,
                version: None,
                output_sample: None,
                error_message: Some(format!("Unknown tool id: {tool_id}")),
            },
        }
    }

    /// No renderer that turns a script or prompt into finished media is wired
    /// into OpenAccounts, so jobs are refused instead of faking progress.
    pub async fn create_production_job(&self, spec: CreativeAssetSpec) -> Result<String> {
        let tools = self.tools.read().await;
        let missing: Vec<&str> = spec
            .tools_required
            .iter()
            .filter(|t| !tools.get(t.as_str()).map(|c| c.is_available).unwrap_or(false))
            .map(String::as_str)
            .collect();
        let missing_note = if missing.is_empty() {
            String::new()
        } else {
            format!(" Required tools not available on this host: {}.", missing.join(", "))
        };
        Err(OpenAccountsError::NotConfigured(format!(
            "No creative render backend is connected, so asset '{}' cannot be produced. \
             Studio media is rendered by the StickExplainers pipeline, not by OpenAccounts.{missing_note}",
            spec.asset_id
        )))
    }

    pub async fn get_job_status(&self, job_id: &str) -> Option<CreativeJobStatus> {
        self.jobs.read().await.get(job_id).cloned()
    }

    /// Inspects a real media file with ffprobe (dimensions, duration, audio)
    /// and measures integrated loudness with ffmpeg's ebur128 filter.
    pub async fn validate_media(
        &self,
        file_path: &str,
        expected_ratio: MediaAspectRatio,
    ) -> Result<MediaValidationResult> {
        if !std::path::Path::new(file_path).is_file() {
            return Err(OpenAccountsError::Internal(format!("Media file not found: {file_path}")));
        }

        let probe = run_tool(
            "ffprobe",
            &["-v", "error", "-print_format", "json", "-show_streams", "-show_format", file_path],
            FFPROBE_TIMEOUT,
        )
        .await?;
        if !probe.success {
            return Err(OpenAccountsError::Internal(format!(
                "ffprobe could not read {file_path}: {}",
                first_line(&probe.stderr).unwrap_or_default()
            )));
        }
        let info: serde_json::Value = serde_json::from_str(&probe.stdout)?;
        let streams = info["streams"].as_array().cloned().unwrap_or_default();

        let video = streams.iter().find(|s| s["codec_type"] == "video");
        let has_audio = streams.iter().any(|s| s["codec_type"] == "audio");
        let width = video.and_then(|v| v["width"].as_u64()).map(|w| w as u32);
        let height = video.and_then(|v| v["height"].as_u64()).map(|h| h as u32);
        let duration_seconds = info["format"]["duration"]
            .as_str()
            .and_then(|d| d.parse::<f32>().ok());

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let aspect_ratio = match (width, height) {
            (Some(w), Some(h)) => MediaAspectRatio::classify(w, h),
            _ => {
                errors.push("No video stream found".to_string());
                None
            }
        };
        if let (Some(w), Some(h)) = (width, height) {
            if aspect_ratio != Some(expected_ratio) {
                errors.push(format!("Frame is {w}x{h}; expected {expected_ratio:?}"));
            }
        }

        let audio_lufs = if has_audio {
            let measured = measure_lufs(file_path).await;
            if measured.is_none() {
                warnings.push("Loudness could not be measured".to_string());
            }
            measured
        } else {
            warnings.push("File has no audio stream".to_string());
            None
        };

        Ok(MediaValidationResult {
            is_valid: errors.is_empty(),
            width,
            height,
            aspect_ratio,
            duration_seconds,
            audio_lufs,
            has_audio,
            errors,
            warnings,
        })
    }
}

/// Integrated loudness ("I: -14.2 LUFS" in the ebur128 summary).
async fn measure_lufs(file_path: &str) -> Option<f32> {
    let out = run_tool(
        "ffmpeg",
        &["-hide_banner", "-nostats", "-i", file_path, "-af", "ebur128", "-f", "null", "-"],
        LOUDNESS_TIMEOUT,
    )
    .await
    .ok()?;
    if !out.success {
        return None;
    }
    out.stderr
        .lines()
        .rev()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("I:"))
        .and_then(|rest| rest.trim().trim_end_matches("LUFS").trim().parse::<f32>().ok())
}

impl Default for CreativePipeline {
    fn default() -> Self {
        Self::new()
    }
}
