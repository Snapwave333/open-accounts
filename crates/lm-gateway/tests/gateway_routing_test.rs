use async_trait::async_trait;
use openaccounts_core::{OpenAccountsError, Result};
use openaccounts_lm_gateway::{
    load_stick_nim_cascade, AnthropicAdapter, ChatMessage, CompletionRequest, CompletionResponse, LMGateway,
    ModelAdapter, ModelProfile, ModelProviderType, DEFAULT_PROFILE_ID,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

struct ScriptedAdapter {
    reply: Option<String>,
    calls: AtomicUsize,
    delay: Duration,
}

impl ScriptedAdapter {
    fn ok(reply: &str) -> Arc<Self> {
        Arc::new(Self { reply: Some(reply.into()), calls: AtomicUsize::new(0), delay: Duration::ZERO })
    }

    fn failing() -> Arc<Self> {
        Arc::new(Self { reply: None, calls: AtomicUsize::new(0), delay: Duration::ZERO })
    }

    fn slow(reply: &str, delay: Duration) -> Arc<Self> {
        Arc::new(Self { reply: Some(reply.into()), calls: AtomicUsize::new(0), delay })
    }
}

#[async_trait]
impl ModelAdapter for ScriptedAdapter {
    async fn complete(&self, req: CompletionRequest, _api_key: Option<String>) -> Result<CompletionResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        match &self.reply {
            Some(r) => Ok(CompletionResponse {
                content: format!("{r}:{}", req.max_tokens.unwrap_or(0)),
                model: "scripted".into(),
                tokens_used: Some(42),
            }),
            None => Err(OpenAccountsError::LMGateway("scripted failure".into())),
        }
    }
}

fn profile(id: &str, provider: ModelProviderType) -> ModelProfile {
    ModelProfile {
        id: id.into(),
        name: id.into(),
        provider_type: provider,
        endpoint: None,
        model_id: "test-model".into(),
        api_key_secret_id: None,
        max_tokens: Some(321),
        temperature: Some(0.2),
    }
}

#[tokio::test]
async fn generate_applies_profile_defaults_and_tracks_usage() {
    let gw = LMGateway::new();
    let adapter = ScriptedAdapter::ok("hello");
    gw.register_profile_with_adapter(profile("local", ModelProviderType::Ollama), adapter.clone())
        .await;

    let resp = gw
        .generate("local", CompletionRequest::chat(vec![ChatMessage::user("hi")]), None)
        .await
        .unwrap();
    assert_eq!(resp.content, "hello:321");

    let usage = gw.usage_report().await;
    let local = usage.get("local").unwrap();
    assert_eq!(local.requests, 1);
    assert_eq!(local.failures, 0);
    assert_eq!(local.tokens_used, 42);
}

#[tokio::test]
async fn fallback_chain_skips_failing_profiles() {
    let gw = LMGateway::new();
    let broken = ScriptedAdapter::failing();
    let healthy = ScriptedAdapter::ok("rescued");
    gw.register_profile_with_adapter(profile("broken", ModelProviderType::OpenAICompatible), broken.clone())
        .await;
    gw.register_profile_with_adapter(profile("healthy", ModelProviderType::Ollama), healthy.clone())
        .await;

    let resp = gw
        .generate_with_fallback(&["broken", "healthy"], CompletionRequest::chat(vec![ChatMessage::user("x")]))
        .await
        .unwrap();
    assert!(resp.content.starts_with("rescued"));
    assert_eq!(broken.calls.load(Ordering::SeqCst), 1);
    assert_eq!(healthy.calls.load(Ordering::SeqCst), 1);
    assert_eq!(gw.usage_report().await.get("broken").unwrap().failures, 1);
}

#[tokio::test]
async fn quick_complete_honours_timeout() {
    let gw = LMGateway::new();
    gw.register_profile_with_adapter(
        profile("sluggish", ModelProviderType::Ollama),
        ScriptedAdapter::slow("late", Duration::from_secs(5)),
    )
    .await;
    let err = gw
        .quick_complete("sluggish", "sys", "user", 64, Duration::from_millis(50))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("timed out"));
}

#[tokio::test]
async fn registering_does_not_block_while_generation_in_flight() {
    let gw = Arc::new(LMGateway::new());
    gw.register_profile_with_adapter(
        profile("slow", ModelProviderType::Ollama),
        ScriptedAdapter::slow("done", Duration::from_millis(400)),
    )
    .await;

    let in_flight = {
        let gw = gw.clone();
        tokio::spawn(async move {
            gw.generate("slow", CompletionRequest::chat(vec![ChatMessage::user("x")]), None)
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;

    let register = tokio::time::timeout(
        Duration::from_millis(200),
        gw.register_profile(profile("another", ModelProviderType::Ollama)),
    )
    .await;
    assert!(register.is_ok(), "profile registration must not wait for in-flight completions");
    assert!(in_flight.await.unwrap().is_ok());
}

#[tokio::test]
async fn default_profile_cannot_be_removed() {
    let gw = LMGateway::new();
    assert!(!gw.remove_profile(DEFAULT_PROFILE_ID).await);
    gw.register_profile(profile("temp", ModelProviderType::Gemini)).await;
    assert!(gw.has_profile("temp").await);
    assert!(gw.remove_profile("temp").await);
    assert!(!gw.has_profile("temp").await);
}

#[test]
fn provider_defaults_point_at_real_endpoints() {
    assert!(ModelProviderType::Anthropic.default_endpoint().contains("api.anthropic.com"));
    assert!(ModelProviderType::Gemini.default_endpoint().contains("generativelanguage.googleapis.com"));
    assert!(ModelProviderType::OpenRouter.default_endpoint().contains("openrouter.ai"));
    assert_eq!(ModelProviderType::Anthropic.default_api_key_env(), Some("ANTHROPIC_API_KEY"));
}

#[test]
fn anthropic_body_lifts_system_and_merges_consecutive_turns() {
    let adapter = AnthropicAdapter::new("https://api.anthropic.com".into(), "claude-test".into());
    let req = CompletionRequest {
        prompt: Some("final question".into()),
        messages: vec![
            ChatMessage::system("be brief"),
            ChatMessage::user("first"),
            ChatMessage::assistant("ack"),
            ChatMessage::user("second"),
        ],
        max_tokens: Some(200),
        temperature: Some(1.7),
        stop: vec!["END".into()],
    };
    let body = adapter.build_body(&req);
    assert_eq!(body["system"], "be brief");
    assert_eq!(body["max_tokens"], 200);
    assert_eq!(body["temperature"], 1.0);
    assert_eq!(body["stop_sequences"][0], "END");
    let turns = body["messages"].as_array().unwrap();
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[2]["role"], "user");
    assert_eq!(turns[2]["content"], "second\n\nfinal question");
}

#[test]
fn cascade_file_ranks_first_and_drops_dead_models() {
    let dir = std::env::temp_dir().join(format!("oa-cascade-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cascade.json");
    std::fs::write(
        &path,
        r#"{"at":0,"text":["custom/a","custom/b","custom/a"],"vision":[],"dead":["openai/gpt-oss-20b"]}"#,
    )
    .unwrap();
    let models = load_stick_nim_cascade(Some(&path), "text");
    assert_eq!(&models[..2], &["custom/a".to_string(), "custom/b".to_string()]);
    assert_eq!(models.iter().filter(|m| *m == "custom/a").count(), 1);
    assert!(!models.contains(&"openai/gpt-oss-20b".to_string()));
    std::fs::remove_dir_all(&dir).ok();
}
