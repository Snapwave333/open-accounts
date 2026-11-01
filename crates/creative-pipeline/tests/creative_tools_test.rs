use openaccounts_creative_pipeline::{
    CreativeAssetSpec, CreativePipeline, MediaAspectRatio,
};

#[tokio::test]
async fn test_creative_tools_discovery_and_launch() {
    let pipeline = CreativePipeline::new();
    pipeline.discover_tools().await;

    let tools = pipeline.list_tools().await;
    assert_eq!(tools.len(), 6, "Expected 6 discovered creative tools");

    // 1. Verify FFmpeg CLI
    let ffmpeg_ver = pipeline.verify_cli_tool("ffmpeg").await;
    assert!(ffmpeg_ver.is_installed);
    assert_eq!(ffmpeg_ver.version.as_deref(), Some("9.0.2"));
    assert!(ffmpeg_ver.error_message.is_none());

    // 2. Verify Blender
    let blender_ver = pipeline.verify_cli_tool("blender").await;
    assert!(blender_ver.is_installed);
    assert_eq!(blender_ver.version.as_deref(), Some("5.1.2"));

    // 3. Verify Vector Craft CLI
    let vc_ver = pipeline.verify_cli_tool("vectorcraft").await;
    assert!(vc_ver.is_installed);
    assert_eq!(vc_ver.version.as_deref(), Some("0.11.0"));

    // 4. Verify Photo Craft CLI
    let pc_ver = pipeline.verify_cli_tool("photocraft").await;
    assert!(pc_ver.is_installed);
    assert_eq!(pc_ver.version.as_deref(), Some("0.9.0"));

    // 5. Verify Effect Craft CLI
    let ec_ver = pipeline.verify_cli_tool("effectcraft").await;
    assert!(ec_ver.is_installed);
    assert_eq!(ec_ver.version.as_deref(), Some("0.10.0"));

    // 6. Verify OpenToonz
    let ot_ver = pipeline.verify_cli_tool("opentoonz").await;
    assert!(ot_ver.is_installed);
    assert_eq!(ot_ver.version.as_deref(), Some("1.5.0"));

    // 7. Verify failure handling for unknown or missing tool
    let invalid_tool = pipeline.verify_cli_tool("nonexistent_tool_cli").await;
    assert!(!invalid_tool.is_installed);
    assert!(invalid_tool.error_message.is_some());
}

#[tokio::test]
async fn test_creative_job_execution_and_output_validation() {
    let pipeline = CreativePipeline::new();

    let spec = CreativeAssetSpec {
        asset_id: "reel-security-01".into(),
        title: "Security Stored Offline".into(),
        output_format: "mp4".into(),
        target_ratio: MediaAspectRatio::Vertical9x16,
        target_platforms: vec!["instagram".into(), "tiktok".into()],
        script_or_prompt: "Protecting 50 accounts with offline vault".into(),
        brand_id: "brand-core".into(),
        tools_required: vec!["ffmpeg".into(), "vectorcraft".into()],
    };

    let job_id = pipeline.create_production_job(spec).await.expect("Job creation failed");
    let status = pipeline.execute_job(&job_id).await.expect("Job execution failed");

    assert_eq!(status.status, "Completed");
    assert_eq!(status.progress_percent, 100);
    assert!(status.output_path.is_some());

    // Validate media quality
    let val = status.validation.expect("Missing media validation");
    assert!(val.is_valid);
    assert_eq!(val.aspect_ratio, MediaAspectRatio::Vertical9x16);
    assert_eq!(val.audio_lufs, -14.2);
    assert!(val.has_audio);
    assert!(val.errors.is_empty());
}
