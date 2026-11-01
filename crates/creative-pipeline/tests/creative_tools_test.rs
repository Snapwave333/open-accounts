use openaccounts_creative_pipeline::{
    normalize_hex_color, parse_ffprobe_json, parse_loudnorm_input_i, parse_tool_version, wrap_text, CreativeAssetSpec,
    CreativePipeline, MediaAspectRatio, ARTIFACTS_DIR_ENV, JOB_COMPLETED, JOB_FAILED, JOB_QUEUED, SILENCE_LUFS,
};
use std::path::Path;

fn tool_installed(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn spec(asset: &str, ratio: MediaAspectRatio, format: &str) -> CreativeAssetSpec {
    CreativeAssetSpec {
        asset_id: asset.into(),
        title: "Security Stored Offline".into(),
        output_format: format.into(),
        target_ratio: ratio,
        target_platforms: vec!["instagram".into(), "tiktok".into()],
        script_or_prompt: "Protecting 50 accounts with an offline vault: zeroized buffers, Argon2id, AES-256-GCM.".into(),
        brand_id: "brand-core".into(),
        tools_required: vec!["ffmpeg".into()],
        background_hex: Some("#141722".into()),
        duration_seconds: Some(2.0),
    }
}

#[tokio::test]
async fn discovery_reports_real_host_state() {
    let pipeline = CreativePipeline::new();
    pipeline.discover_tools().await;
    let tools = pipeline.list_tools().await;
    assert!(tools.len() >= 8);

    let ffmpeg = tools.iter().find(|t| t.command == "ffmpeg").unwrap();
    assert_eq!(ffmpeg.is_available, tool_installed("ffmpeg"));
    if ffmpeg.is_available {
        assert!(ffmpeg.version.is_some());
        let verified = pipeline.verify_cli_tool("ffmpeg").await;
        assert!(verified.is_installed);
        assert!(verified.output_sample.unwrap().to_lowercase().contains("ffmpeg"));
    }

    let missing = pipeline.verify_cli_tool("vectorcraft").await;
    if !missing.is_installed {
        assert!(missing.error_message.is_some());
        assert!(missing.version.is_none());
    }

    let unknown = pipeline.verify_cli_tool("nonexistent_tool_cli").await;
    assert!(!unknown.is_installed);
    assert!(unknown.error_message.is_some());
}

#[test]
fn pure_helpers_behave() {
    assert_eq!(parse_tool_version("ffmpeg version 6.1.1-3ubuntu5 Copyright"), Some("6.1.1-3ubuntu5".into()));
    assert_eq!(parse_tool_version("Blender 4.2.1 LTS\nbuild date"), Some("4.2.1".into()));
    assert_eq!(normalize_hex_color(Some("#d946ef")).unwrap(), "0xD946EF");
    assert!(normalize_hex_color(Some("red")).is_err());
    assert_eq!(wrap_text("one two three four five", 9), "one two\nthree\nfour five");
    assert_eq!(MediaAspectRatio::classify(1080, 1920), Some(MediaAspectRatio::Vertical9x16));
    assert_eq!(MediaAspectRatio::classify(1000, 1000), Some(MediaAspectRatio::Square1x1));
    assert_eq!(MediaAspectRatio::classify(800, 600), None);

    let stderr = "[Parsed_loudnorm_0 @ 0x1]\n{\n\t\"input_i\" : \"-23.40\",\n\t\"input_tp\" : \"-1.00\"\n}\n";
    assert_eq!(parse_loudnorm_input_i(stderr), Some(-23.4));
    assert_eq!(parse_loudnorm_input_i("{\"input_i\" : \"-inf\"}"), Some(SILENCE_LUFS));

    let probe = r#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"side_data_list":[{"rotation":-90}]},{"codec_type":"audio"}],"format":{"duration":"12.5"}}"#;
    let summary = parse_ffprobe_json(probe).unwrap();
    assert_eq!((summary.width, summary.height), (1080, 1920));
    assert!(summary.has_audio);
    assert_eq!(summary.duration_seconds, 12.5);
}

#[tokio::test]
async fn real_render_and_validation_of_a_short_vertical_clip() {
    if !tool_installed("ffmpeg") || !tool_installed("ffprobe") {
        eprintln!("ffmpeg/ffprobe not installed; skipping real render test");
        return;
    }
    let dir = std::env::temp_dir().join(format!("oa-creative-test-{}", std::process::id()));
    std::env::set_var(ARTIFACTS_DIR_ENV, &dir);

    let pipeline = CreativePipeline::new();
    let job_id = pipeline
        .create_production_job(spec("reel-security-01", MediaAspectRatio::Vertical9x16, "mp4"))
        .await
        .unwrap();
    assert_eq!(pipeline.get_job_status(&job_id).await.unwrap().status, JOB_QUEUED);

    let status = pipeline.execute_job(&job_id).await.unwrap();
    assert_eq!(status.status, JOB_COMPLETED, "job failed: {:?}", status.error_message);
    assert_eq!(status.progress_percent, 100);
    let output = status.output_path.clone().unwrap();
    assert!(Path::new(&output).is_file());
    assert!(output.starts_with(dir.to_string_lossy().as_ref()));

    let validation = status.validation.unwrap();
    assert!(validation.is_valid, "{:?}", validation.errors);
    assert_eq!(validation.aspect_ratio, MediaAspectRatio::Vertical9x16);
    assert_eq!((validation.width, validation.height), (1080, 1920));
    assert!((validation.duration_seconds - 2.0).abs() < 0.3);
    assert!(validation.has_audio);
    assert!(validation.warnings.iter().any(|w| w.contains("silent")));

    let mismatch = pipeline
        .validate_media(&output, MediaAspectRatio::Landscape16x9)
        .await
        .unwrap();
    assert!(!mismatch.is_valid);

    let missing = pipeline
        .validate_media("/definitely/not/here.mp4", MediaAspectRatio::Vertical9x16)
        .await
        .unwrap();
    assert!(!missing.is_valid);

    let bad_format = pipeline
        .create_production_job(spec("bad", MediaAspectRatio::Square1x1, "gif"))
        .await
        .unwrap();
    let failed = pipeline.execute_job(&bad_format).await.unwrap();
    assert_eq!(failed.status, JOB_FAILED);
    assert!(failed.error_message.unwrap().contains("Unsupported"));

    std::fs::remove_dir_all(&dir).ok();
}
