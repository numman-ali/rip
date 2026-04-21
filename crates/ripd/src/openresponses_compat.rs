use crate::provider_openresponses::{
    OpenResponsesInclude, OpenResponsesReasoningConfig, ReasoningEffort, ReasoningSummary,
};
use rip_provider_openresponses::ValidationOptions;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompatLevel {
    Native,
    Compat,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStrategy {
    PreviousResponseId,
    StatelessHistory,
    ConfigDriven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct ConversationSupport {
    pub previous_response_id: CompatLevel,
    pub stateless_history: CompatLevel,
    pub recommended: ConversationStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct ModelCapabilityHealth {
    pub reasoning_parameter: CompatLevel,
    pub tool_calling: CompatLevel,
    pub structured_outputs: CompatLevel,
    pub input_modalities: ModalityCapabilityHealth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct RequestCapabilityHealth {
    pub background: CompatLevel,
    pub store: CompatLevel,
    pub service_tier: CompatLevel,
    pub response_include: CompatLevel,
    pub reasoning_parameter: CompatLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct ToolCapabilityHealth {
    pub function_calling: CompatLevel,
    pub tool_choice: CompatLevel,
    pub allowed_tools: CompatLevel,
    pub hosted_tools: CompatLevel,
    pub mcp_servers: CompatLevel,
    pub mcp_headers: CompatLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct ModalityCapabilityHealth {
    pub input_text: CompatLevel,
    pub input_image: CompatLevel,
    pub input_file: CompatLevel,
    pub input_video: CompatLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct OpenResponsesReasoningSupport {
    pub parameter: CompatLevel,
    pub effort: CompatLevel,
    pub summary: CompatLevel,
    pub supported_efforts: Vec<ReasoningEffort>,
    pub supported_summaries: Vec<ReasoningSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ResolvedOpenResponsesReasoning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested: Option<OpenResponsesReasoningConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective: Option<OpenResponsesReasoningConfig>,
    pub support: OpenResponsesReasoningSupport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct OpenResponsesIncludeSupport {
    pub request: CompatLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ResolvedOpenResponsesInclude {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested: Vec<OpenResponsesInclude>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effective: Vec<OpenResponsesInclude>,
    pub support: OpenResponsesIncludeSupport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ResolvedOpenResponsesConversation {
    pub requested: ConversationStrategy,
    pub effective: ConversationStrategy,
    pub support: ConversationSupport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl ResolvedOpenResponsesCompatProfile {
    pub fn validation_options(self, requested_stateless_history: bool) -> ValidationOptions {
        self.effective_validation(requested_stateless_history)
            .to_validation_options()
    }

    pub fn effective_validation(self, requested_stateless_history: bool) -> ValidationProfile {
        let mut validation = self.provider.validation;
        if matches!(
            self.conversation(requested_stateless_history).effective,
            ConversationStrategy::StatelessHistory
        ) {
            validation.missing_item_ids = true;
        }
        validation
    }

    pub fn active_conversation_strategy(
        self,
        requested_stateless_history: bool,
    ) -> ConversationStrategy {
        self.conversation(requested_stateless_history).effective
    }

    pub fn conversation(
        self,
        requested_stateless_history: bool,
    ) -> ResolvedOpenResponsesConversation {
        let support = self.provider.conversation;
        let requested = requested_conversation_strategy(requested_stateless_history);
        let mut effective = requested;
        let mut warnings = Vec::new();
        let requested_level = conversation_support_level(support, requested);

        if requested_level == CompatLevel::Unsupported {
            if let Some(alternate) = alternate_conversation_strategy(requested) {
                let alternate_level = conversation_support_level(support, alternate);
                if alternate_level != CompatLevel::Unsupported {
                    effective = alternate;
                    warnings.push(format!(
                        "{} does not support {}; using {} instead.",
                        route_label(self),
                        conversation_strategy_label(requested),
                        conversation_strategy_label(effective)
                    ));
                }
            }
        }

        ResolvedOpenResponsesConversation {
            requested,
            effective,
            support,
            warnings,
        }
    }

    pub fn reasoning(
        self,
        requested: Option<&OpenResponsesReasoningConfig>,
    ) -> ResolvedOpenResponsesReasoning {
        let support = self.reasoning_support();
        let requested = requested
            .cloned()
            .and_then(OpenResponsesReasoningConfig::normalized);
        let mut effective = requested.clone().unwrap_or_default();
        let mut warnings = Vec::new();

        if support.parameter == CompatLevel::Unsupported {
            if requested.is_some() {
                warnings.push(format!(
                    "{} does not support the OpenResponses reasoning parameter; omitting reasoning.",
                    route_label(self)
                ));
            }
            return ResolvedOpenResponsesReasoning {
                requested,
                effective: None,
                support,
                warnings,
            };
        }

        if let Some(effort) = effective.effort {
            if !support.supported_efforts.is_empty() && !support.supported_efforts.contains(&effort)
            {
                warnings.push(format!(
                    "reasoning.effort={} is not supported on {}; omitting effort.",
                    reasoning_effort_label(effort),
                    route_label(self)
                ));
                effective.effort = None;
            } else if support.effort == CompatLevel::Unknown {
                warnings.push(format!(
                    "reasoning.effort={} is unverified on {}; forwarding as requested.",
                    reasoning_effort_label(effort),
                    route_label(self)
                ));
            }
        }

        if let Some(summary) = effective.summary {
            if !support.supported_summaries.is_empty()
                && !support.supported_summaries.contains(&summary)
            {
                warnings.push(format!(
                    "reasoning.summary={} is not supported on {}; omitting summary.",
                    reasoning_summary_label(summary),
                    route_label(self)
                ));
                effective.summary = None;
            } else if support.summary == CompatLevel::Unknown {
                warnings.push(format!(
                    "reasoning.summary={} is unverified on {}; forwarding as requested.",
                    reasoning_summary_label(summary),
                    route_label(self)
                ));
            }
        }

        ResolvedOpenResponsesReasoning {
            requested,
            effective: effective.normalized(),
            support,
            warnings,
        }
    }

    pub fn include(self, requested: &[OpenResponsesInclude]) -> ResolvedOpenResponsesInclude {
        let support = OpenResponsesIncludeSupport {
            request: self.provider.request.response_include,
        };
        let requested = requested.to_vec();
        let mut warnings = Vec::new();

        let effective = match support.request {
            CompatLevel::Unsupported => {
                if !requested.is_empty() {
                    warnings.push(format!(
                        "{} does not support OpenResponses include selections; omitting include.",
                        route_label(self)
                    ));
                }
                Vec::new()
            }
            CompatLevel::Unknown => {
                if !requested.is_empty() {
                    warnings.push(format!(
                        "include is unverified on {}; forwarding as requested.",
                        route_label(self)
                    ));
                }
                requested.clone()
            }
            CompatLevel::Native | CompatLevel::Compat => requested.clone(),
        };

        ResolvedOpenResponsesInclude {
            requested,
            effective,
            support,
            warnings,
        }
    }

    pub fn reasoning_support(self) -> OpenResponsesReasoningSupport {
        let rule = reasoning_support_rule(self);
        OpenResponsesReasoningSupport {
            parameter: rule.parameter,
            effort: rule.effort,
            summary: rule.summary,
            supported_efforts: rule.supported_efforts.to_vec(),
            supported_summaries: rule.supported_summaries.to_vec(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReasoningSupportRule {
    parameter: CompatLevel,
    effort: CompatLevel,
    summary: CompatLevel,
    supported_efforts: &'static [ReasoningEffort],
    supported_summaries: &'static [ReasoningSummary],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
pub struct ValidationProfile {
    pub missing_item_ids: bool,
    pub missing_response_user: bool,
    pub reasoning_text_events: bool,
    pub missing_reasoning_summary: bool,
}

impl ValidationProfile {
    const STRICT: Self = Self {
        missing_item_ids: false,
        missing_response_user: false,
        reasoning_text_events: false,
        missing_reasoning_summary: false,
    };

    const OPENROUTER: Self = Self {
        missing_item_ids: true,
        missing_response_user: true,
        reasoning_text_events: true,
        missing_reasoning_summary: true,
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
        if self.missing_reasoning_summary {
            options = options.with_missing_reasoning_summary();
        }
        options
    }
}

pub const OPENRESPONSES_COMPAT_PROFILE_VERSION: &str = "2026-04-19.v1";

const OPENAI_REASONING_SUMMARIES: &[ReasoningSummary] = &[
    ReasoningSummary::Auto,
    ReasoningSummary::Concise,
    ReasoningSummary::Detailed,
];

const OPENAI_GPT_54_REASONING_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::None,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::Xhigh,
];

const OPENROUTER_REASONING_EFFORTS: &[ReasoningEffort] = &[
    ReasoningEffort::Minimal,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

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

const OPENROUTER_GEMMA_4_26B_A4B_IT: OpenResponsesModelCompatProfile =
    OpenResponsesModelCompatProfile {
        version: OPENRESPONSES_COMPAT_PROFILE_VERSION,
        provider_id: "openrouter",
        model_id: "google/gemma-4-26b-a4b-it",
        label: "Google Gemma 4 26B A4B IT",
        health: ModelCapabilityHealth {
            reasoning_parameter: CompatLevel::Native,
            tool_calling: CompatLevel::Unknown,
            structured_outputs: CompatLevel::Unknown,
            input_modalities: TEXT_ONLY_MODALITIES,
        },
    };

const OPENROUTER_NEMOTRON_3_SUPER_120B_A12B_FREE: OpenResponsesModelCompatProfile =
    OpenResponsesModelCompatProfile {
        version: OPENRESPONSES_COMPAT_PROFILE_VERSION,
        provider_id: "openrouter",
        model_id: "nvidia/nemotron-3-super-120b-a12b:free",
        label: "NVIDIA Nemotron 3 Super 120B A12B (free)",
        health: ModelCapabilityHealth {
            reasoning_parameter: CompatLevel::Native,
            tool_calling: CompatLevel::Unknown,
            structured_outputs: CompatLevel::Unknown,
            input_modalities: TEXT_ONLY_MODALITIES,
        },
    };

const OPENAI_GPT_5_4_NANO: OpenResponsesModelCompatProfile = OpenResponsesModelCompatProfile {
    version: OPENRESPONSES_COMPAT_PROFILE_VERSION,
    provider_id: "openai",
    model_id: "gpt-5.4-nano",
    label: "GPT-5.4 nano",
    health: ModelCapabilityHealth {
        reasoning_parameter: CompatLevel::Native,
        tool_calling: CompatLevel::Native,
        structured_outputs: CompatLevel::Native,
        input_modalities: ModalityCapabilityHealth {
            input_text: CompatLevel::Native,
            input_image: CompatLevel::Native,
            input_file: CompatLevel::Native,
            input_video: CompatLevel::Unsupported,
        },
    },
};

const OPENAI_GPT_5_4_MINI: OpenResponsesModelCompatProfile = OpenResponsesModelCompatProfile {
    version: OPENRESPONSES_COMPAT_PROFILE_VERSION,
    provider_id: "openai",
    model_id: "gpt-5.4-mini",
    label: "GPT-5.4 mini",
    health: ModelCapabilityHealth {
        reasoning_parameter: CompatLevel::Native,
        tool_calling: CompatLevel::Native,
        structured_outputs: CompatLevel::Native,
        input_modalities: ModalityCapabilityHealth {
            input_text: CompatLevel::Native,
            input_image: CompatLevel::Native,
            input_file: CompatLevel::Native,
            input_video: CompatLevel::Unsupported,
        },
    },
};

const OPENAI_GPT_5_NANO: OpenResponsesModelCompatProfile = OpenResponsesModelCompatProfile {
    version: OPENRESPONSES_COMPAT_PROFILE_VERSION,
    provider_id: "openai",
    model_id: "gpt-5-nano",
    label: "GPT-5 nano",
    health: ModelCapabilityHealth {
        reasoning_parameter: CompatLevel::Native,
        tool_calling: CompatLevel::Native,
        structured_outputs: CompatLevel::Native,
        input_modalities: ModalityCapabilityHealth {
            input_text: CompatLevel::Native,
            input_image: CompatLevel::Native,
            input_file: CompatLevel::Native,
            input_video: CompatLevel::Unsupported,
        },
    },
};

const OPENAI_MODEL_PROFILES: &[OpenResponsesModelCompatProfile] =
    &[OPENAI_GPT_5_4_NANO, OPENAI_GPT_5_4_MINI, OPENAI_GPT_5_NANO];

const OPENROUTER_MODEL_PROFILES: &[OpenResponsesModelCompatProfile] = &[
    OPENROUTER_NEMOTRON_3_NANO_30B_A3B_FREE,
    OPENROUTER_GEMMA_4_26B_A4B_IT,
    OPENROUTER_NEMOTRON_3_SUPER_120B_A12B_FREE,
];

pub fn resolve_openresponses_compat_profile(
    provider_id: Option<&str>,
    endpoint: &str,
    model: Option<&str>,
) -> ResolvedOpenResponsesCompatProfile {
    let provider = provider_id
        .and_then(resolve_provider_profile_by_id)
        .unwrap_or_else(|| {
            if crate::provider_openresponses::is_openrouter_responses_endpoint(endpoint) {
                &OPENROUTER_PROVIDER_PROFILE
            } else if is_openai_responses_endpoint(endpoint) {
                &OPENAI_PROVIDER_PROFILE
            } else {
                &GENERIC_PROVIDER_PROFILE
            }
        });

    let model = model.and_then(|model| resolve_model_profile(provider.provider_id, model));
    ResolvedOpenResponsesCompatProfile { provider, model }
}

fn resolve_provider_profile_by_id(
    provider_id: &str,
) -> Option<&'static OpenResponsesProviderCompatProfile> {
    match provider_id.trim() {
        "openai" => Some(&OPENAI_PROVIDER_PROFILE),
        "openrouter" => Some(&OPENROUTER_PROVIDER_PROFILE),
        "generic" => Some(&GENERIC_PROVIDER_PROFILE),
        _ => None,
    }
}

fn resolve_model_profile(
    provider_id: &str,
    model: &str,
) -> Option<&'static OpenResponsesModelCompatProfile> {
    match provider_id {
        "openai" => OPENAI_MODEL_PROFILES
            .iter()
            .find(|profile| profile.model_id == model),
        "openrouter" => OPENROUTER_MODEL_PROFILES
            .iter()
            .find(|profile| profile.model_id == model),
        _ => None,
    }
}

fn reasoning_support_rule(resolved: ResolvedOpenResponsesCompatProfile) -> ReasoningSupportRule {
    let provider_parameter = resolved
        .model
        .map(|model| model.health.reasoning_parameter)
        .unwrap_or(resolved.provider.request.reasoning_parameter);

    match (
        resolved.provider.provider_id,
        resolved.model.map(|model| model.model_id),
    ) {
        ("openai", Some("gpt-5.4-nano" | "gpt-5.4-mini")) => ReasoningSupportRule {
            parameter: provider_parameter,
            effort: CompatLevel::Native,
            summary: CompatLevel::Native,
            supported_efforts: OPENAI_GPT_54_REASONING_EFFORTS,
            supported_summaries: OPENAI_REASONING_SUMMARIES,
        },
        ("openrouter", Some("google/gemma-4-26b-a4b-it")) => ReasoningSupportRule {
            parameter: provider_parameter,
            effort: CompatLevel::Native,
            summary: CompatLevel::Compat,
            supported_efforts: OPENROUTER_REASONING_EFFORTS,
            supported_summaries: OPENAI_REASONING_SUMMARIES,
        },
        ("openrouter", _) => ReasoningSupportRule {
            parameter: provider_parameter,
            effort: CompatLevel::Native,
            summary: CompatLevel::Unknown,
            supported_efforts: OPENROUTER_REASONING_EFFORTS,
            supported_summaries: &[],
        },
        ("openai", _) => ReasoningSupportRule {
            parameter: provider_parameter,
            effort: CompatLevel::Unknown,
            summary: CompatLevel::Unknown,
            supported_efforts: &[],
            supported_summaries: &[],
        },
        _ => ReasoningSupportRule {
            parameter: provider_parameter,
            effort: CompatLevel::Unknown,
            summary: CompatLevel::Unknown,
            supported_efforts: &[],
            supported_summaries: &[],
        },
    }
}

fn reasoning_effort_label(value: ReasoningEffort) -> &'static str {
    match value {
        ReasoningEffort::None => "none",
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::Xhigh => "xhigh",
    }
}

fn reasoning_summary_label(value: ReasoningSummary) -> &'static str {
    match value {
        ReasoningSummary::Auto => "auto",
        ReasoningSummary::Concise => "concise",
        ReasoningSummary::Detailed => "detailed",
    }
}

fn requested_conversation_strategy(stateless_history: bool) -> ConversationStrategy {
    if stateless_history {
        ConversationStrategy::StatelessHistory
    } else {
        ConversationStrategy::PreviousResponseId
    }
}

fn alternate_conversation_strategy(strategy: ConversationStrategy) -> Option<ConversationStrategy> {
    match strategy {
        ConversationStrategy::PreviousResponseId => Some(ConversationStrategy::StatelessHistory),
        ConversationStrategy::StatelessHistory => Some(ConversationStrategy::PreviousResponseId),
        ConversationStrategy::ConfigDriven => None,
    }
}

fn conversation_support_level(
    support: ConversationSupport,
    strategy: ConversationStrategy,
) -> CompatLevel {
    match strategy {
        ConversationStrategy::PreviousResponseId => support.previous_response_id,
        ConversationStrategy::StatelessHistory => support.stateless_history,
        ConversationStrategy::ConfigDriven => CompatLevel::Unknown,
    }
}

fn conversation_strategy_label(value: ConversationStrategy) -> &'static str {
    match value {
        ConversationStrategy::PreviousResponseId => "previous_response_id",
        ConversationStrategy::StatelessHistory => "stateless_history",
        ConversationStrategy::ConfigDriven => "config_driven",
    }
}

fn route_label(resolved: ResolvedOpenResponsesCompatProfile) -> String {
    match (
        resolved.provider.provider_id,
        resolved.model.map(|model| model.model_id),
    ) {
        (provider_id, Some(model_id)) => format!("{provider_id}/{model_id}"),
        (provider_id, None) => provider_id.to_string(),
    }
}

fn is_openai_responses_endpoint(endpoint: &str) -> bool {
    let raw = endpoint.trim();
    raw == "https://api.openai.com/v1/responses" || raw == "https://api.openai.com/v1/responses/"
}

#[cfg(test)]
mod tests;
