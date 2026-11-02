use openaccounts_creative_pipeline::{CreativeAssetSpec, CreativePipeline, MediaAspectRatio};

#[tokio::test]
async fn test_tool_discovery_reports_real_probe_results() {
    let pipeline = CreativePipeline::new();
    pipeline.discover_tools().await;

    let tools = pipeline.list_tools().await;
    assert!(!tools.is_empty(), "every known tool must be listed with its probe result");
    for tool in &tools {
        if tool.is_available {
            assert!(tool.version.is_some(), "{} is available but reported no version", tool.name);
            assert!(tool.probe_error.is_none());
        } else {
            assert!(tool.probe_error.is_some(), "{} is unavailable without a reason", tool.name);
        }
    }

    let unknown = pipeline.verify_cli_tool("nonexistent_tool_cli").await;
    assert!(!unknown.is_installed);
    assert!(unknown.error_message.is_some());
}

#[tokio::test]
async fn test_creative_jobs_are_refused_without_a_render_backend() {
    let pipeline = CreativePipeline::new();
    let spec = CreativeAssetSpec {
        asset_id: "reel-01".into(),
        title: "Test".into(),
        output_format: "mp4".into(),
        target_ratio: MediaAspectRatio::Vertical9x16,
        target_platforms: vec!["instagram".into()],
        script_or_prompt: "test".into(),
        brand_id: "brand_cdn".into(),
        tools_required: vec!["ffmpeg".into()],
    };

    let err = pipeline.create_production_job(spec).await.expect_err("must not fake a job");
    assert!(err.to_string().contains("Not configured"));
}

#[test]
fn test_aspect_ratio_classification() {
    assert_eq!(MediaAspectRatio::classify(1080, 1920), Some(MediaAspectRatio::Vertical9x16));
    assert_eq!(MediaAspectRatio::classify(1080, 1080), Some(MediaAspectRatio::Square1x1));
    assert_eq!(MediaAspectRatio::classify(1920, 1080), Some(MediaAspectRatio::Landscape16x9));
    assert_eq!(MediaAspectRatio::classify(1000, 1300), None);
}

#[tokio::test]
async fn test_validate_media_rejects_missing_file() {
    let pipeline = CreativePipeline::new();
    let res = pipeline
        .validate_media("Z:/definitely/not/here.mp4", MediaAspectRatio::Vertical9x16)
        .await;
    assert!(res.is_err());
}

/// Renders a 1s 9:16 clip with ffmpeg and validates it for real. Skipped when
/// ffmpeg/ffprobe are not installed.
#[tokio::test]
async fn test_validate_media_on_real_clip() {
    let pipeline = CreativePipeline::new();
    if !pipeline.verify_cli_tool("ffmpeg").await.is_installed
        || !pipeline.verify_cli_tool("ffprobe").await.is_installed
    {
        eprintln!("ffmpeg/ffprobe not installed; skipping");
        return;
    }

    let dir = std::env::temp_dir().join(format!("oa-creative-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let clip = dir.join("clip.mp4");
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner", "-loglevel", "error", "-y",
            "-f", "lavfi", "-i", "testsrc=size=1080x1920:rate=30:duration=1",
            "-f", "lavfi", "-i", "sine=frequency=440:duration=1",
            "-shortest", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac",
        ])
        .arg(&clip)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg could not render the test clip");

    let result = pipeline
        .validate_media(clip.to_str().unwrap(), MediaAspectRatio::Vertical9x16)
        .await
        .expect("validation runs");
    let mismatch = pipeline
        .validate_media(clip.to_str().unwrap(), MediaAspectRatio::Landscape16x9)
        .await
        .expect("validation runs");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(!mismatch.is_valid, "a 9:16 clip must fail a 16:9 check");

    assert!(result.is_valid, "errors: {:?}", result.errors);
    assert_eq!(result.width, Some(1080));
    assert_eq!(result.height, Some(1920));
    assert_eq!(result.aspect_ratio, Some(MediaAspectRatio::Vertical9x16));
    assert!(result.has_audio);
    let duration = result.duration_seconds.expect("duration reported");
    assert!((duration - 1.0).abs() < 0.2, "duration was {duration}");
    assert!(result.audio_lufs.is_some(), "loudness should be measured");
}
