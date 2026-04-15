use super::AuthRequestTelemetryContext;
use super::GITHUB_COPILOT_INITIATOR_HEADER;
use super::GITHUB_COPILOT_INTENT_HEADER;
use super::GITHUB_COPILOT_PROVIDER_NAME;
use super::GITHUB_COPILOT_VISION_HEADER;
use super::ModelClient;
use super::PendingUnauthorizedRetry;
use super::UnauthorizedRecoveryExecution;
use super::build_provider_responses_headers;
use super::infer_github_copilot_initiator;
use codex_api::api_bridge::CoreAuthProvider;
use codex_app_server_protocol::AuthMode;
use codex_model_provider_info::WireApi;
use codex_model_provider_info::create_oss_provider_with_base_url;
use codex_otel::SessionTelemetry;
use codex_protocol::ThreadId;
use codex_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;

use crate::RequestInitiator;
use serde_json::json;

fn test_model_client(session_source: SessionSource) -> ModelClient {
    let provider = create_oss_provider_with_base_url("https://example.com/v1", WireApi::Responses);
    ModelClient::new(
        /*auth_manager*/ None,
        ThreadId::new(),
        provider,
        session_source,
        /*model_verbosity*/ None,
        /*enable_request_compression*/ false,
        /*include_timing_metrics*/ false,
        /*beta_features_header*/ None,
    )
}

fn test_model_info() -> ModelInfo {
    serde_json::from_value(json!({
        "slug": "gpt-test",
        "display_name": "gpt-test",
        "description": "desc",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": [
            {"effort": "medium", "description": "medium"}
        ],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 1,
        "upgrade": null,
        "base_instructions": "base instructions",
        "model_messages": null,
        "supports_reasoning_summaries": false,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "truncation_policy": {"mode": "bytes", "limit": 10000},
        "supports_parallel_tool_calls": false,
        "supports_image_detail_original": false,
        "context_window": 272000,
        "auto_compact_token_limit": null,
        "experimental_supported_tools": []
    }))
    .expect("deserialize test model info")
}

fn test_session_telemetry() -> SessionTelemetry {
    SessionTelemetry::new(
        ThreadId::new(),
        "gpt-test",
        "gpt-test",
        /*account_id*/ None,
        /*account_email*/ None,
        /*auth_mode*/ None,
        "test-originator".to_string(),
        /*log_user_prompts*/ false,
        "test-terminal".to_string(),
        SessionSource::Cli,
    )
}

fn test_github_copilot_provider() -> codex_model_provider_info::ModelProviderInfo {
    let mut provider =
        create_oss_provider_with_base_url("https://api.githubcopilot.com/v1", WireApi::Responses);
    provider.name = GITHUB_COPILOT_PROVIDER_NAME.to_string();
    provider
}

#[test]
fn build_subagent_headers_sets_other_subagent_label() {
    let client = test_model_client(SessionSource::SubAgent(SubAgentSource::Other(
        "memory_consolidation".to_string(),
    )));
    let headers = client.build_subagent_headers();
    let value = headers
        .get("x-openai-subagent")
        .and_then(|value| value.to_str().ok());
    assert_eq!(value, Some("memory_consolidation"));
}

#[tokio::test]
async fn summarize_memories_returns_empty_for_empty_input() {
    let client = test_model_client(SessionSource::Cli);
    let model_info = test_model_info();
    let session_telemetry = test_session_telemetry();

    let output = client
        .summarize_memories(
            Vec::new(),
            &model_info,
            /*effort*/ None,
            &session_telemetry,
        )
        .await
        .expect("empty summarize request should succeed");
    assert_eq!(output.len(), 0);
}

#[test]
fn auth_request_telemetry_context_tracks_attached_auth_and_retry_phase() {
    let auth_context = AuthRequestTelemetryContext::new(
        Some(AuthMode::Chatgpt),
        &CoreAuthProvider::for_test(Some("access-token"), Some("workspace-123")),
        PendingUnauthorizedRetry::from_recovery(UnauthorizedRecoveryExecution {
            mode: "managed",
            phase: "refresh_token",
        }),
    );

    assert_eq!(auth_context.auth_mode, Some("Chatgpt"));
    assert!(auth_context.auth_header_attached);
    assert_eq!(auth_context.auth_header_name, Some("authorization"));
    assert!(auth_context.retry_after_unauthorized);
    assert_eq!(auth_context.recovery_mode, Some("managed"));
    assert_eq!(auth_context.recovery_phase, Some("refresh_token"));
}

#[test]
fn infer_github_copilot_initiator_uses_last_item_role() {
    let user_input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "hello".to_string(),
        }],
        end_turn: None,
        phase: None,
    }];
    assert_eq!(infer_github_copilot_initiator(&user_input), "user");

    let tool_output = vec![ResponseItem::FunctionCallOutput {
        call_id: "call-1".to_string(),
        output: FunctionCallOutputPayload::from_text("done".to_string()),
    }];
    assert_eq!(infer_github_copilot_initiator(&tool_output), "agent");
}

#[test]
fn build_provider_responses_headers_adds_copilot_headers_for_images_and_tool_outputs() {
    let provider = test_github_copilot_provider();
    let input = vec![
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text: "check this".to_string(),
                },
                ContentItem::InputImage {
                    image_url: "data:image/png;base64,abc".to_string(),
                },
            ],
            end_turn: None,
            phase: None,
        },
        ResponseItem::CustomToolCallOutput {
            call_id: "call-1".to_string(),
            name: Some("tool".to_string()),
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputImage {
                    image_url: "data:image/png;base64,def".to_string(),
                    detail: None,
                },
            ]),
        },
    ];

    let headers = build_provider_responses_headers(
        &provider,
        &input,
        &SessionSource::Cli,
        RequestInitiator::Auto,
    );

    assert_eq!(
        headers
            .get(GITHUB_COPILOT_INTENT_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some("conversation-edits")
    );
    assert_eq!(
        headers
            .get(GITHUB_COPILOT_INITIATOR_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some("agent")
    );
    assert_eq!(
        headers
            .get(GITHUB_COPILOT_VISION_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );
}

#[test]
fn build_provider_responses_headers_skips_non_copilot_providers() {
    let provider = create_oss_provider_with_base_url("https://example.com/v1", WireApi::Responses);
    let headers = build_provider_responses_headers(
        &provider,
        &[],
        &SessionSource::Cli,
        RequestInitiator::Auto,
    );
    assert_eq!(headers.len(), 0);
}

#[test]
fn build_provider_responses_headers_accepts_copilot_base_url_even_with_custom_name() {
    let mut provider = create_oss_provider_with_base_url(
        "https://api.individual.githubcopilot.com/v1",
        WireApi::Responses,
    );
    provider.name = "Custom Provider Name".to_string();

    let headers = build_provider_responses_headers(
        &provider,
        &[],
        &SessionSource::Cli,
        RequestInitiator::Auto,
    );
    assert_eq!(
        headers
            .get(GITHUB_COPILOT_INTENT_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some("conversation-edits")
    );
}

#[test]
fn build_provider_responses_headers_marks_subagent_bootstrap_requests_as_agent() {
    let provider = test_github_copilot_provider();
    let input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "delegated task".to_string(),
        }],
        end_turn: None,
        phase: None,
    }];

    let headers = build_provider_responses_headers(
        &provider,
        &input,
        &SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: ThreadId::new(),
            depth: 1,
            agent_path: Default::default(),
            agent_nickname: None,
            agent_role: None,
        }),
        RequestInitiator::Auto,
    );

    assert_eq!(
        headers
            .get(GITHUB_COPILOT_INITIATOR_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some("agent")
    );
}

#[test]
fn build_provider_responses_headers_respects_explicit_agent_initiator() {
    let provider = test_github_copilot_provider();
    let input = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "synthetic prompt".to_string(),
        }],
        end_turn: None,
        phase: None,
    }];

    let headers = build_provider_responses_headers(
        &provider,
        &input,
        &SessionSource::Cli,
        RequestInitiator::Agent,
    );

    assert_eq!(
        headers
            .get(GITHUB_COPILOT_INITIATOR_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some("agent")
    );
}

#[test]
fn reset_websocket_session_preserves_turn_state_token() {
    let client = test_model_client(SessionSource::Cli);
    let mut session = client.new_session();
    assert!(session.turn_state.set("sticky-token".to_string()).is_ok());

    session.reset_websocket_session();

    assert_eq!(
        session.turn_state.get().map(String::as_str),
        Some("sticky-token")
    );
}

#[test]
fn reset_turn_continuity_clears_turn_state_token() {
    let client = test_model_client(SessionSource::Cli);
    let mut session = client.new_session();
    assert!(session.turn_state.set("old-token".to_string()).is_ok());

    session.reset_turn_continuity();

    assert!(session.turn_state.get().is_none());
    assert!(session.turn_state.set("new-token".to_string()).is_ok());
    assert_eq!(
        session.turn_state.get().map(String::as_str),
        Some("new-token")
    );
}

#[test]
fn reset_turn_continuity_disables_prompt_cache_key() {
    let client = test_model_client(SessionSource::Cli);
    let mut session = client.new_session();
    let provider = client
        .state
        .provider
        .to_api_provider(/*auth_mode*/ None)
        .expect("provider should convert to API provider");
    let model_info = test_model_info();
    let prompt = crate::Prompt {
        input: Vec::new(),
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: "base instructions".to_string(),
        },
        personality: None,
        request_initiator: RequestInitiator::Auto,
        output_schema: None,
    };

    let request_before_reset = session
        .build_responses_request(
            &provider,
            &prompt,
            &model_info,
            /*effort*/ None,
            ReasoningSummaryConfig::None,
            /*service_tier*/ None,
        )
        .expect("request before continuity reset");
    assert_eq!(
        request_before_reset.prompt_cache_key,
        Some(client.state.conversation_id.to_string())
    );

    session.reset_turn_continuity();

    let request_after_reset = session
        .build_responses_request(
            &provider,
            &prompt,
            &model_info,
            /*effort*/ None,
            ReasoningSummaryConfig::None,
            /*service_tier*/ None,
        )
        .expect("request after continuity reset");
    assert_eq!(request_after_reset.prompt_cache_key, None);
}
