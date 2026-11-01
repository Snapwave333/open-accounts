use openaccounts_lm_gateway::{
    load_stick_nim_cascade, strip_think, LMGateway, ModelProviderType, NvidiaNimCascadeAdapter,
};

#[tokio::test]
async fn test_stickexplainers_nim_cascade_loaded_and_configured() {
    let text_cascade = load_stick_nim_cascade(None, "text");
    assert!(
        text_cascade.len() >= 10,
        "Expected full StickExplainers NIM text cascade, got {}",
        text_cascade.len()
    );
    assert_eq!(text_cascade[0], "nvidia/nemotron-3.5-super-vl-120b-a12b");
    assert!(text_cascade.contains(&"meta/llama-3.2-90b-vision-instruct".to_string()));
    assert!(text_cascade.contains(&"meta/muse-glimmer-30b".to_string()));
    assert!(text_cascade.contains(&"nvidia/nemotron-3.5-lightning-30b-a3b".to_string()));

    let vision_cascade = load_stick_nim_cascade(None, "vision");
    assert!(!vision_cascade.is_empty());
    assert_eq!(vision_cascade[0], "nvidia/nemotron-3.5-super-vl-120b-a12b");

    let raw = "<think>\nLet me analyze the prompt...\n</think>\nCDN OK";
    assert_eq!(strip_think(raw), "CDN OK");

    let adapter = NvidiaNimCascadeAdapter::new(None, "text", None);
    let initial_order = adapter.ordered_models().await;
    let first = initial_order[0].clone();

    adapter.record_health(&first, false, 1.2, "HTTP 429", Some(60)).await;
    let next_order = adapter.ordered_models().await;
    assert_ne!(
        next_order[0], first,
        "Model with open breaker must be skipped in cascade order"
    );

    let gw = LMGateway::new();
    let profiles = gw.list_profiles().await;
    assert!(profiles.iter().any(|p| p.id == "nim-cascade" && p.provider_type == ModelProviderType::NvidiaNim));
    assert!(profiles.iter().any(|p| p.id == "nim-vision-cascade" && p.provider_type == ModelProviderType::NvidiaNim));
}
