use rip_provider_openresponses::ValidationOptions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatLevel {
    Native,
    Compat,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationStrategy {
    PreviousResponseId,
    StatelessHistory,
    ConfigDriven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversationSupport {
    pub previous_response_id: CompatLevel,
    pub stateless_history: CompatLevel,
    pub recommended: ConversationStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelCapabilityHealth {
    pub reasoning_parameter: CompatLevel,
    pub tool_calling: CompatLevel,
    pub structured_outputs: CompatLevel,
    pub input_modalities: ModalityCapabilityHealth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestCapabilityHealth {
    pub background: CompatLevel,
    pub store: CompatLevel,
    pub service_tier: CompatLevel,
    pub response_include: CompatLevel,
    pub reasoning_parameter: CompatLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolCapabilityHealth {
    pub function_calling: CompatLevel,
    pub tool_choice: CompatLevel,
    pub allowed_tools: CompatLevel,
    pub hosted_tools: CompatLevel,
    pub mcp_servers: CompatLevel,
    pub mcp_headers: CompatLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModalityCapabilityHealth {
    pub input_text: CompatLevel,
    pub input_image: CompatLevel,
    pub input_file: CompatLevel,
    pub input_video: CompatLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenResponsesProviderCompatProfile {
    pub version: &'static str,
    pub provider_id: &'static str,
    pub label: &'static str,
    pub stream_shape: CompatLevel,
    pub conversation: ConversationSupport,
    pub request: RequestCapabilityHealth,
    pub tools: ToolCapabilityHealth,
    pub input_modalities: ModalityCapabilityHealth,
    pub validation: ValidationProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenResponsesModelCompatProfile {
    pub version: &'static str,
    pub provider_id: &'static str,
    pub model_id: &'static str,
    pub label: &'static str,
    pub health: ModelCapabilityHealth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedOpenResponsesCompatProfile {
    pub provider: &'static OpenResponsesProviderCompatProfile,
    pub model: Option<&'static OpenResponsesModelCompatProfile>,
}

impl ResolvedOpenResponsesCompatProfile {
    pub fn validation_options(self, stateless_history: bool) -> ValidationOptions {
        let mut validation = self.provider.validation.to_validation_options();
        if stateless_history {
            validation = validation.with_missing_item_ids();
        }
        validation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationProfile {
    pub missing_item_ids: bool,
    pub missing_response_user: bool,
    pub reasoning_text_events: bool,
}

impl ValidationProfile {
    const STRICT: Self = Self {
        missing_item_ids: false,
        missing_response_user: false,
        reasoning_text_events: false,
    };

    const OPENROUTER: Self = Self {
        missing_item_ids: true,
        missing_response_user: true,
        reasoning_text_events: true,
    };

    fn to_validation_options(self) -> ValidationOptions {
        let mut options = ValidationOptions::strict();
        if self.missing_item_ids {
            options = options.with_missing_item_ids();
        }
        if self.missing_response_user {
            options = options.with_missing_response_user();
        }
        if self.reasoning_text_events {
            options = options.with_reasoning_text_events();
        }
        options
    }
}

pub const OPENRESPONSES_COMPAT_PROFILE_VERSION: &str = "2026-04-19.v1";

const UNKNOWN_REQUEST_CAPABILITIES: RequestCapabilityHealth = RequestCapabilityHealth {
    background: CompatLevel::Unknown,
    store: CompatLevel::Unknown,
    service_tier: CompatLevel::Unknown,
    response_include: CompatLevel::Unknown,
    reasoning_parameter: CompatLevel::Unknown,
};

const UNKNOWN_TOOL_CAPABILITIES: ToolCapabilityHealth = ToolCapabilityHealth {
    function_calling: CompatLevel::Unknown,
    tool_choice: CompatLevel::Unknown,
    allowed_tools: CompatLevel::Unknown,
    hosted_tools: CompatLevel::Unknown,
    mcp_servers: CompatLevel::Unknown,
    mcp_headers: CompatLevel::Unknown,
};

const TEXT_ONLY_MODALITIES: ModalityCapabilityHealth = ModalityCapabilityHealth {
    input_text: CompatLevel::Native,
    input_image: CompatLevel::Unknown,
    input_file: CompatLevel::Unknown,
    input_video: CompatLevel::Unknown,
};

const GENERIC_PROVIDER_PROFILE: OpenResponsesProviderCompatProfile =
    OpenResponsesProviderCompatProfile {
        version: OPENRESPONSES_COMPAT_PROFILE_VERSION,
        provider_id: "generic",
        label: "Generic OpenResponses-compatible endpoint",
        stream_shape: CompatLevel::Unknown,
        conversation: ConversationSupport {
            previous_response_id: CompatLevel::Unknown,
            stateless_history: CompatLevel::Unknown,
            recommended: ConversationStrategy::ConfigDriven,
        },
        request: UNKNOWN_REQUEST_CAPABILITIES,
        tools: UNKNOWN_TOOL_CAPABILITIES,
        input_modalities: TEXT_ONLY_MODALITIES,
        validation: ValidationProfile::STRICT,
    };

const OPENAI_PROVIDER_PROFILE: OpenResponsesProviderCompatProfile =
    OpenResponsesProviderCompatProfile {
        version: OPENRESPONSES_COMPAT_PROFILE_VERSION,
        provider_id: "openai",
        label: "OpenAI Responses API",
        stream_shape: CompatLevel::Native,
        conversation: ConversationSupport {
            previous_response_id: CompatLevel::Native,
            stateless_history: CompatLevel::Native,
            recommended: ConversationStrategy::PreviousResponseId,
        },
        request: RequestCapabilityHealth {
            background: CompatLevel::Native,
            store: CompatLevel::Native,
            service_tier: CompatLevel::Native,
            response_include: CompatLevel::Native,
            reasoning_parameter: CompatLevel::Native,
        },
        tools: ToolCapabilityHealth {
            function_calling: CompatLevel::Native,
            tool_choice: CompatLevel::Native,
            allowed_tools: CompatLevel::Native,
            hosted_tools: CompatLevel::Unknown,
            mcp_servers: CompatLevel::Unknown,
            mcp_headers: CompatLevel::Unknown,
        },
        input_modalities: ModalityCapabilityHealth {
            input_text: CompatLevel::Native,
            input_image: CompatLevel::Native,
            input_file: CompatLevel::Native,
            input_video: CompatLevel::Unknown,
        },
        validation: ValidationProfile::STRICT,
    };

const OPENROUTER_PROVIDER_PROFILE: OpenResponsesProviderCompatProfile =
    OpenResponsesProviderCompatProfile {
        version: OPENRESPONSES_COMPAT_PROFILE_VERSION,
        provider_id: "openrouter",
        label: "OpenRouter Responses API Beta",
        stream_shape: CompatLevel::Compat,
        conversation: ConversationSupport {
            previous_response_id: CompatLevel::Unsupported,
            stateless_history: CompatLevel::Native,
            recommended: ConversationStrategy::StatelessHistory,
        },
        request: RequestCapabilityHealth {
            background: CompatLevel::Unknown,
            store: CompatLevel::Unsupported,
            service_tier: CompatLevel::Unknown,
            response_include: CompatLevel::Unknown,
            reasoning_parameter: CompatLevel::Native,
        },
        tools: ToolCapabilityHealth {
            function_calling: CompatLevel::Native,
            tool_choice: CompatLevel::Native,
            allowed_tools: CompatLevel::Unknown,
            hosted_tools: CompatLevel::Compat,
            mcp_servers: CompatLevel::Unknown,
            mcp_headers: CompatLevel::Unknown,
        },
        input_modalities: TEXT_ONLY_MODALITIES,
        validation: ValidationProfile::OPENROUTER,
    };

const OPENROUTER_NEMOTRON_3_NANO_30B_A3B_FREE: OpenResponsesModelCompatProfile =
    OpenResponsesModelCompatProfile {
        version: OPENRESPONSES_COMPAT_PROFILE_VERSION,
        provider_id: "openrouter",
        model_id: "nvidia/nemotron-3-nano-30b-a3b:free",
        label: "NVIDIA Nemotron 3 Nano 30B A3B (free)",
        health: ModelCapabilityHealth {
            reasoning_parameter: CompatLevel::Native,
            tool_calling: CompatLevel::Unknown,
            structured_outputs: CompatLevel::Unknown,
            input_modalities: TEXT_ONLY_MODALITIES,
        },
    };

const OPENROUTER_MODEL_PROFILES: &[OpenResponsesModelCompatProfile] =
    &[OPENROUTER_NEMOTRON_3_NANO_30B_A3B_FREE];

pub fn resolve_openresponses_compat_profile(
    endpoint: &str,
    model: Option<&str>,
) -> ResolvedOpenResponsesCompatProfile {
    let provider = if crate::provider_openresponses::is_openrouter_responses_endpoint(endpoint) {
        &OPENROUTER_PROVIDER_PROFILE
    } else if is_openai_responses_endpoint(endpoint) {
        &OPENAI_PROVIDER_PROFILE
    } else {
        &GENERIC_PROVIDER_PROFILE
    };

    let model = model.and_then(|model| resolve_model_profile(provider.provider_id, model));
    ResolvedOpenResponsesCompatProfile { provider, model }
}

fn resolve_model_profile(
    provider_id: &str,
    model: &str,
) -> Option<&'static OpenResponsesModelCompatProfile> {
    match provider_id {
        "openrouter" => OPENROUTER_MODEL_PROFILES
            .iter()
            .find(|profile| profile.model_id == model),
        _ => None,
    }
}

fn is_openai_responses_endpoint(endpoint: &str) -> bool {
    let raw = endpoint.trim();
    raw == "https://api.openai.com/v1/responses" || raw == "https://api.openai.com/v1/responses/"
}

#[cfg(test)]
mod tests;
