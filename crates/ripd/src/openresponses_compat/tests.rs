use super::*;

#[test]
fn resolves_openai_profile_strictly() {
    let resolved = resolve_openresponses_compat_profile(
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
    let resolved =
        resolve_openresponses_compat_profile("https://example.test/v1/responses", Some("foo"));

    assert_eq!(resolved.provider.provider_id, "generic");
    assert_eq!(resolved.provider.validation, ValidationProfile::STRICT);
    assert!(resolved.model.is_none());
}
