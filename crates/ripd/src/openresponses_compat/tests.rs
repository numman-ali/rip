use super::*;

#[test]
fn resolves_openai_profile_strictly() {
    let resolved = resolve_openresponses_compat_profile(
        None,
        "https://api.openai.com/v1/responses",
        Some("gpt-5-nano-2025-08-07"),
    );

    assert_eq!(resolved.provider.provider_id, "openai");
    assert_eq!(resolved.provider.validation, ValidationProfile::STRICT);
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
        ValidationOptions::compat_missing_item_ids()
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
