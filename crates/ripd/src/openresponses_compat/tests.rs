use super::*;
use crate::provider_openresponses::{
    OpenResponsesApproximateLocation, OpenResponsesInclude, OpenResponsesWebSearchConfig,
    SearchContextSize,
};

#[test]
fn resolves_openai_profile_strictly() {
    let resolved = resolve_openresponses_compat_profile(
        None,
        "https://api.openai.com/v1/responses",
        Some("gpt-5-nano-2025-08-07"),
    );

    assert_eq!(resolved.provider.provider_id, "openai");
    assert_eq!(resolved.provider.validation, ValidationProfile::OPENAI);
    assert_eq!(
        resolved.provider.conversation.recommended,
        ConversationStrategy::PreviousResponseId
    );
    assert_eq!(resolved.provider.request.background, CompatLevel::Native);
    assert_eq!(resolved.provider.request.store, CompatLevel::Native);
    assert_eq!(
        resolved.provider.tools.function_calling,
        CompatLevel::Native
    );
    assert_eq!(resolved.provider.tools.allowed_tools, CompatLevel::Native);
    assert_eq!(
        resolved.provider.input_modalities.input_image,
        CompatLevel::Native
    );
    assert!(resolved.model.is_none());
}

#[test]
fn resolves_openrouter_profile_with_compat_validation() {
    let resolved = resolve_openresponses_compat_profile(
        None,
        "https://openrouter.ai/api/v1/responses",
        Some("openai/gpt-oss-20b"),
    );

    assert_eq!(resolved.provider.provider_id, "openrouter");
    assert_eq!(resolved.provider.stream_shape, CompatLevel::Compat);
    assert_eq!(resolved.provider.validation, ValidationProfile::OPENROUTER);
    assert_eq!(
        resolved.provider.conversation.previous_response_id,
        CompatLevel::Unsupported
    );
    assert_eq!(
        resolved.provider.conversation.recommended,
        ConversationStrategy::StatelessHistory
    );
    assert_eq!(resolved.provider.request.store, CompatLevel::Unsupported);
    assert_eq!(
        resolved.provider.request.reasoning_parameter,
        CompatLevel::Native
    );
    assert_eq!(
        resolved.provider.tools.function_calling,
        CompatLevel::Native
    );
    assert_eq!(resolved.provider.tools.hosted_tools, CompatLevel::Compat);
}

#[test]
fn stateless_history_adds_missing_item_id_normalization_even_for_strict_profiles() {
    let resolved = resolve_openresponses_compat_profile(
        None,
        "https://api.openai.com/v1/responses",
        Some("gpt-5-nano-2025-08-07"),
    );

    assert_eq!(
        resolved.validation_options(true),
        ValidationOptions::compat_missing_item_ids().with_response_web_search_tools()
    );
}

#[test]
fn openrouter_model_overlay_matches_nemotron_free() {
    let resolved = resolve_openresponses_compat_profile(
        None,
        "https://openrouter.ai/api/v1/responses",
        Some("nvidia/nemotron-3-nano-30b-a3b:free"),
    );

    let model = resolved.model.expect("model profile");
    assert_eq!(model.provider_id, "openrouter");
    assert_eq!(model.model_id, "nvidia/nemotron-3-nano-30b-a3b:free");
    assert_eq!(model.health.reasoning_parameter, CompatLevel::Native);
    assert_eq!(model.health.tool_calling, CompatLevel::Unknown);
    assert_eq!(
        model.health.input_modalities.input_image,
        CompatLevel::Unknown
    );
}

#[test]
fn unknown_endpoint_resolves_generic_profile() {
    let resolved = resolve_openresponses_compat_profile(
        None,
        "https://example.test/v1/responses",
        Some("foo"),
    );

    assert_eq!(resolved.provider.provider_id, "generic");
    assert_eq!(resolved.provider.validation, ValidationProfile::STRICT);
    assert!(resolved.model.is_none());
}

#[test]
fn provider_id_takes_precedence_over_noncanonical_endpoint() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openrouter"),
        "http://127.0.0.1:4010/v1/responses",
        Some("nvidia/nemotron-3-nano-30b-a3b:free"),
    );

    assert_eq!(resolved.provider.provider_id, "openrouter");
    assert_eq!(resolved.provider.validation, ValidationProfile::OPENROUTER);
    assert_eq!(
        resolved.provider.conversation.recommended,
        ConversationStrategy::StatelessHistory
    );
    assert_eq!(
        resolved.model.map(|model| model.model_id),
        Some("nvidia/nemotron-3-nano-30b-a3b:free")
    );
}

#[test]
fn explicit_generic_provider_id_uses_generic_profile_even_on_known_endpoint() {
    let resolved = resolve_openresponses_compat_profile(
        Some("generic"),
        "https://api.openai.com/v1/responses",
        Some("gpt-5.4-mini"),
    );

    assert_eq!(resolved.provider.provider_id, "generic");
    assert_eq!(resolved.provider.validation, ValidationProfile::STRICT);
    assert!(resolved.model.is_none());
}

#[test]
fn openrouter_conversation_coerces_unsupported_previous_response_id_to_stateless_history() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openrouter"),
        "https://openrouter.ai/api/v1/responses",
        Some("nvidia/nemotron-3-nano-30b-a3b:free"),
    );

    let conversation = resolved.conversation(false);

    assert_eq!(
        conversation.requested,
        ConversationStrategy::PreviousResponseId
    );
    assert_eq!(
        conversation.effective,
        ConversationStrategy::StatelessHistory
    );
    assert_eq!(
        conversation.support.previous_response_id,
        CompatLevel::Unsupported
    );
    assert!(conversation
        .warnings
        .iter()
        .any(|warning| warning.contains("does not support previous_response_id")));
}

#[test]
fn openai_conversation_preserves_explicit_stateless_history() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openai"),
        "https://api.openai.com/v1/responses",
        Some("gpt-5.4-nano"),
    );

    let conversation = resolved.conversation(true);

    assert_eq!(
        conversation.requested,
        ConversationStrategy::StatelessHistory
    );
    assert_eq!(
        conversation.effective,
        ConversationStrategy::StatelessHistory
    );
    assert_eq!(conversation.support.stateless_history, CompatLevel::Native);
    assert!(conversation.warnings.is_empty());
}

#[test]
fn active_conversation_strategy_reports_effective_strategy() {
    let openai = resolve_openresponses_compat_profile(
        Some("openai"),
        "https://api.openai.com/v1/responses",
        Some("gpt-5.4-mini"),
    );
    let openrouter = resolve_openresponses_compat_profile(
        Some("openrouter"),
        "https://openrouter.ai/api/v1/responses",
        Some("google/gemma-4-26b-a4b-it"),
    );

    assert_eq!(
        openai.active_conversation_strategy(false),
        ConversationStrategy::PreviousResponseId
    );
    assert_eq!(
        openrouter.active_conversation_strategy(false),
        ConversationStrategy::StatelessHistory
    );
}

#[test]
fn generic_conversation_keeps_requested_strategy_without_warning() {
    let resolved = resolve_openresponses_compat_profile(
        Some("generic"),
        "https://provider.example.test/v1/responses",
        Some("model-x"),
    );

    let conversation = resolved.conversation(false);

    assert_eq!(
        conversation.requested,
        ConversationStrategy::PreviousResponseId
    );
    assert_eq!(
        conversation.effective,
        ConversationStrategy::PreviousResponseId
    );
    assert_eq!(
        conversation.support.previous_response_id,
        CompatLevel::Unknown
    );
    assert!(conversation.warnings.is_empty());
}

#[test]
fn openai_gpt_54_models_constrain_reasoning_effort_and_keep_supported_summary() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openai"),
        "https://api.openai.com/v1/responses",
        Some("gpt-5.4-nano"),
    );

    let reasoning = resolved.reasoning(Some(&OpenResponsesReasoningConfig {
        effort: Some(ReasoningEffort::Minimal),
        summary: Some(ReasoningSummary::Detailed),
    }));

    assert_eq!(reasoning.support.parameter, CompatLevel::Native);
    assert_eq!(reasoning.support.effort, CompatLevel::Native);
    assert_eq!(reasoning.support.summary, CompatLevel::Native);
    assert_eq!(
        reasoning.support.supported_efforts,
        OPENAI_GPT_54_REASONING_EFFORTS.to_vec()
    );
    assert_eq!(
        reasoning.effective,
        Some(OpenResponsesReasoningConfig {
            effort: None,
            summary: Some(ReasoningSummary::Detailed),
        })
    );
    assert!(reasoning
        .warnings
        .iter()
        .any(|warning| warning.contains("reasoning.effort=minimal")));
}

#[test]
fn openai_unknown_model_forwards_reasoning_with_unverified_warnings() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openai"),
        "https://api.openai.com/v1/responses",
        Some("gpt-future"),
    );

    let reasoning = resolved.reasoning(Some(&OpenResponsesReasoningConfig {
        effort: Some(ReasoningEffort::High),
        summary: Some(ReasoningSummary::Auto),
    }));

    assert_eq!(reasoning.support.parameter, CompatLevel::Native);
    assert_eq!(reasoning.support.effort, CompatLevel::Unknown);
    assert_eq!(reasoning.support.summary, CompatLevel::Unknown);
    assert_eq!(
        reasoning.effective,
        Some(OpenResponsesReasoningConfig {
            effort: Some(ReasoningEffort::High),
            summary: Some(ReasoningSummary::Auto),
        })
    );
    assert!(reasoning
        .warnings
        .iter()
        .any(|warning| warning.contains("reasoning.effort=high is unverified")));
    assert!(reasoning
        .warnings
        .iter()
        .any(|warning| warning.contains("reasoning.summary=auto is unverified")));
}

#[test]
fn absent_reasoning_request_stays_absent_without_warning() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openai"),
        "https://api.openai.com/v1/responses",
        Some("gpt-5.4-mini"),
    );

    let reasoning = resolved.reasoning(None);

    assert_eq!(reasoning.requested, None);
    assert_eq!(reasoning.effective, None);
    assert!(reasoning.warnings.is_empty());
}

#[test]
fn openrouter_generic_route_drops_unsupported_effort_and_flags_summary_unverified() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openrouter"),
        "https://openrouter.ai/api/v1/responses",
        Some("nvidia/nemotron-3-super-120b-a12b:free"),
    );

    let reasoning = resolved.reasoning(Some(&OpenResponsesReasoningConfig {
        effort: Some(ReasoningEffort::Xhigh),
        summary: Some(ReasoningSummary::Detailed),
    }));

    assert_eq!(reasoning.support.parameter, CompatLevel::Native);
    assert_eq!(reasoning.support.effort, CompatLevel::Native);
    assert_eq!(reasoning.support.summary, CompatLevel::Unknown);
    assert_eq!(
        reasoning.support.supported_efforts,
        OPENROUTER_REASONING_EFFORTS.to_vec()
    );
    assert_eq!(
        reasoning.effective,
        Some(OpenResponsesReasoningConfig {
            effort: None,
            summary: Some(ReasoningSummary::Detailed),
        })
    );
    assert!(reasoning
        .warnings
        .iter()
        .any(|warning| warning.contains("reasoning.effort=xhigh")));
    assert!(reasoning
        .warnings
        .iter()
        .any(|warning| warning.contains("reasoning.summary=detailed is unverified")));
}

#[test]
fn gemma_route_marks_reasoning_summary_as_compat_supported() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openrouter"),
        "https://openrouter.ai/api/v1/responses",
        Some("google/gemma-4-26b-a4b-it"),
    );

    let reasoning = resolved.reasoning(Some(&OpenResponsesReasoningConfig {
        effort: Some(ReasoningEffort::High),
        summary: Some(ReasoningSummary::Detailed),
    }));

    assert_eq!(reasoning.support.summary, CompatLevel::Compat);
    assert_eq!(
        reasoning.support.supported_summaries,
        OPENAI_REASONING_SUMMARIES.to_vec()
    );
    assert_eq!(
        reasoning.effective,
        Some(OpenResponsesReasoningConfig {
            effort: Some(ReasoningEffort::High),
            summary: Some(ReasoningSummary::Detailed),
        })
    );
    assert!(reasoning.warnings.is_empty());
}

#[test]
fn openai_route_preserves_include_without_warnings() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openai"),
        "https://api.openai.com/v1/responses",
        Some("gpt-5.4-nano"),
    );

    let include = resolved.include(&[
        OpenResponsesInclude::ReasoningEncryptedContent,
        OpenResponsesInclude::MessageOutputTextLogprobs,
    ]);

    assert_eq!(include.support.request, CompatLevel::Native);
    assert_eq!(
        include.support.native_values,
        ALL_OPENRESPONSES_INCLUDE_VALUES.to_vec()
    );
    assert_eq!(
        include.effective,
        vec![
            OpenResponsesInclude::ReasoningEncryptedContent,
            OpenResponsesInclude::MessageOutputTextLogprobs,
        ]
    );
    assert!(include.warnings.is_empty());
}

#[test]
fn openrouter_route_applies_curated_include_subset() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openrouter"),
        "https://openrouter.ai/api/v1/responses",
        Some("google/gemma-4-26b-a4b-it"),
    );

    let include = resolved.include(&[
        OpenResponsesInclude::ReasoningEncryptedContent,
        OpenResponsesInclude::CodeInterpreterCallOutputs,
        OpenResponsesInclude::MessageInputImageImageUrl,
        OpenResponsesInclude::MessageOutputTextLogprobs,
        OpenResponsesInclude::WebSearchCallActionSources,
    ]);

    assert_eq!(include.support.request, CompatLevel::Compat);
    assert_eq!(
        include.support.native_values,
        vec![OpenResponsesInclude::ReasoningEncryptedContent]
    );
    assert_eq!(
        include.support.compat_values,
        vec![
            OpenResponsesInclude::FileSearchCallResults,
            OpenResponsesInclude::CodeInterpreterCallOutputs,
        ]
    );
    assert_eq!(
        include.support.unknown_values,
        vec![
            OpenResponsesInclude::MessageInputImageImageUrl,
            OpenResponsesInclude::ComputerCallOutputOutputImageUrl,
        ]
    );
    assert_eq!(
        include.support.unsupported_values,
        vec![
            OpenResponsesInclude::WebSearchCallResults,
            OpenResponsesInclude::WebSearchCallActionSources,
            OpenResponsesInclude::MessageOutputTextLogprobs,
        ]
    );
    assert_eq!(
        include.effective,
        vec![
            OpenResponsesInclude::ReasoningEncryptedContent,
            OpenResponsesInclude::CodeInterpreterCallOutputs,
            OpenResponsesInclude::MessageInputImageImageUrl,
        ]
    );
    assert!(include
        .warnings
        .iter()
        .any(|warning| warning.contains("include=message.input_image.image_url is unverified")));
    assert!(include
        .warnings
        .iter()
        .any(|warning| warning.contains("include=message.output_text.logprobs is not supported")));
    assert!(
        include
            .warnings
            .iter()
            .any(|warning| warning
                .contains("include=web_search_call.action.sources is not supported"))
    );
}

#[test]
fn generic_route_forwards_all_include_values_as_unverified() {
    let resolved = resolve_openresponses_compat_profile(
        Some("generic"),
        "https://provider.example.test/v1/responses",
        Some("model-x"),
    );

    let include = resolved.include(&[
        OpenResponsesInclude::FileSearchCallResults,
        OpenResponsesInclude::MessageOutputTextLogprobs,
    ]);

    assert_eq!(include.support.request, CompatLevel::Unknown);
    assert_eq!(
        include.effective,
        vec![
            OpenResponsesInclude::FileSearchCallResults,
            OpenResponsesInclude::MessageOutputTextLogprobs,
        ]
    );
    assert_eq!(include.warnings.len(), 2);
    assert!(include
        .warnings
        .iter()
        .all(|warning| warning.contains("is unverified")));
}

#[test]
fn openai_route_preserves_canonical_web_search() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openai"),
        "https://api.openai.com/v1/responses",
        Some("gpt-5.4-nano"),
    );

    let web_search = resolved.web_search(Some(&OpenResponsesWebSearchConfig {
        enabled: true,
        search_context_size: Some(SearchContextSize::High),
        external_web_access: Some(true),
        user_location: Some(OpenResponsesApproximateLocation {
            country: Some("US".to_string()),
            region: Some("California".to_string()),
            city: Some("San Francisco".to_string()),
            timezone: Some("America/Los_Angeles".to_string()),
        }),
    }));

    assert_eq!(web_search.support.request, CompatLevel::Native);
    assert_eq!(web_search.support.search_context_size, CompatLevel::Native);
    assert_eq!(web_search.support.external_web_access, CompatLevel::Native);
    assert_eq!(web_search.support.user_location, CompatLevel::Native);
    assert_eq!(web_search.effective, web_search.requested);
    assert!(web_search.warnings.is_empty());
}

#[test]
fn route_without_web_search_request_does_not_enable_hosted_tool() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openai"),
        "https://api.openai.com/v1/responses",
        Some("gpt-5.4-nano"),
    );

    let web_search = resolved.web_search(None);

    assert_eq!(web_search.support.request, CompatLevel::Native);
    assert_eq!(web_search.requested, None);
    assert_eq!(web_search.effective, None);
    assert!(web_search.warnings.is_empty());
}

#[test]
fn disabled_web_search_request_stays_omitted_without_warning() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openai"),
        "https://api.openai.com/v1/responses",
        Some("gpt-5.4-nano"),
    );

    let web_search = resolved.web_search(Some(&OpenResponsesWebSearchConfig::disabled()));

    assert_eq!(web_search.requested, None);
    assert_eq!(web_search.effective, None);
    assert!(web_search.warnings.is_empty());
}

#[test]
fn generic_route_forwards_web_search_with_unverified_field_warnings() {
    let resolved = resolve_openresponses_compat_profile(
        Some("custom"),
        "https://provider.example.test/v1/responses",
        Some("model-x"),
    );

    let web_search = resolved.web_search(Some(&OpenResponsesWebSearchConfig {
        enabled: true,
        search_context_size: Some(SearchContextSize::Low),
        external_web_access: Some(false),
        user_location: Some(OpenResponsesApproximateLocation {
            country: Some("GB".to_string()),
            region: Some("England".to_string()),
            city: Some("London".to_string()),
            timezone: Some("Europe/London".to_string()),
        }),
    }));

    assert_eq!(web_search.support.request, CompatLevel::Unknown);
    assert_eq!(web_search.effective, web_search.requested);
    assert!(web_search
        .warnings
        .iter()
        .any(|warning| warning.contains("web_search is unverified")));
    assert!(web_search
        .warnings
        .iter()
        .any(|warning| warning.contains("search_context_size is unverified")));
    assert!(web_search
        .warnings
        .iter()
        .any(|warning| warning.contains("external_web_access is unverified")));
    assert!(web_search
        .warnings
        .iter()
        .any(|warning| warning.contains("user_location is unverified")));
}

#[test]
fn openrouter_route_uses_provider_extension_web_search_and_drops_unsupported_external_access() {
    let resolved = resolve_openresponses_compat_profile(
        Some("openrouter"),
        "https://openrouter.ai/api/v1/responses",
        Some("google/gemma-4-26b-a4b-it"),
    );

    let web_search = resolved.web_search(Some(&OpenResponsesWebSearchConfig {
        enabled: true,
        search_context_size: Some(SearchContextSize::Medium),
        external_web_access: Some(true),
        user_location: Some(OpenResponsesApproximateLocation {
            country: Some("US".to_string()),
            region: None,
            city: None,
            timezone: None,
        }),
    }));

    assert_eq!(web_search.support.request, CompatLevel::Compat);
    assert_eq!(web_search.support.search_context_size, CompatLevel::Compat);
    assert_eq!(
        web_search.support.external_web_access,
        CompatLevel::Unsupported
    );
    assert_eq!(web_search.support.user_location, CompatLevel::Compat);
    assert_eq!(
        web_search.effective,
        Some(OpenResponsesWebSearchConfig {
            enabled: true,
            search_context_size: Some(SearchContextSize::Medium),
            external_web_access: None,
            user_location: Some(OpenResponsesApproximateLocation {
                country: Some("US".to_string()),
                region: None,
                city: None,
                timezone: None,
            }),
        })
    );
    assert!(web_search
        .warnings
        .iter()
        .any(|warning| warning.contains("external_web_access is not supported")));
    assert!(!web_search
        .warnings
        .iter()
        .any(|warning| warning.contains("user_location")));
}
