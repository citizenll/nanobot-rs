use crate::providers::base::{LLMProvider, LLMResponse, ToolCallRequest};
use anyhow::Result;
use async_trait::async_trait;
use litellm_rs::core::types::content::ContentPart;
use litellm_rs::core::types::tools::{Tool, ToolChoice};
use litellm_rs::{CompletionOptions, Message, MessageContent, MessageRole, completion};
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Clone, Copy)]
struct ModelOverride {
    pattern: &'static str,
    temperature: Option<f32>,
}

#[derive(Clone, Copy)]
struct EnvExtra {
    key: &'static str,
    value_template: &'static str,
}

#[derive(Clone, Copy)]
struct ProviderSpec {
    name: &'static str,
    keywords: &'static [&'static str],
    env_key: &'static str,
    litellm_prefix: &'static str,
    skip_prefixes: &'static [&'static str],
    is_gateway: bool,
    is_local: bool,
    detect_by_key_prefix: &'static str,
    detect_by_base_keyword: &'static str,
    default_api_base: &'static str,
    strip_model_prefix: bool,
    env_extras: &'static [EnvExtra],
    model_overrides: &'static [ModelOverride],
}

const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        name: "openrouter",
        keywords: &["openrouter"],
        env_key: "OPENROUTER_API_KEY",
        litellm_prefix: "openrouter",
        skip_prefixes: &[],
        is_gateway: true,
        is_local: false,
        detect_by_key_prefix: "sk-or-",
        detect_by_base_keyword: "openrouter",
        default_api_base: "https://openrouter.ai/api/v1",
        strip_model_prefix: false,
        env_extras: &[],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "aihubmix",
        keywords: &["aihubmix"],
        env_key: "OPENAI_API_KEY",
        litellm_prefix: "openai",
        skip_prefixes: &[],
        is_gateway: true,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "aihubmix",
        default_api_base: "https://aihubmix.com/v1",
        strip_model_prefix: true,
        env_extras: &[],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "anthropic",
        keywords: &["anthropic", "claude"],
        env_key: "ANTHROPIC_API_KEY",
        litellm_prefix: "",
        skip_prefixes: &[],
        is_gateway: false,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "",
        strip_model_prefix: false,
        env_extras: &[],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "openai",
        keywords: &["openai", "gpt"],
        env_key: "OPENAI_API_KEY",
        litellm_prefix: "",
        skip_prefixes: &[],
        is_gateway: false,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "",
        strip_model_prefix: false,
        env_extras: &[],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "deepseek",
        keywords: &["deepseek"],
        env_key: "DEEPSEEK_API_KEY",
        litellm_prefix: "deepseek",
        skip_prefixes: &["deepseek/"],
        is_gateway: false,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "",
        strip_model_prefix: false,
        env_extras: &[],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "gemini",
        keywords: &["gemini"],
        env_key: "GEMINI_API_KEY",
        litellm_prefix: "gemini",
        skip_prefixes: &["gemini/"],
        is_gateway: false,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "",
        strip_model_prefix: false,
        env_extras: &[],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "zhipu",
        keywords: &["zhipu", "glm", "zai"],
        env_key: "ZAI_API_KEY",
        litellm_prefix: "zai",
        skip_prefixes: &["zhipu/", "zai/", "openrouter/", "hosted_vllm/"],
        is_gateway: false,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "",
        strip_model_prefix: false,
        env_extras: &[EnvExtra {
            key: "ZHIPUAI_API_KEY",
            value_template: "{api_key}",
        }],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "dashscope",
        keywords: &["qwen", "dashscope"],
        env_key: "DASHSCOPE_API_KEY",
        litellm_prefix: "dashscope",
        skip_prefixes: &["dashscope/", "openrouter/"],
        is_gateway: false,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "",
        strip_model_prefix: false,
        env_extras: &[],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "moonshot",
        keywords: &["moonshot", "kimi"],
        env_key: "MOONSHOT_API_KEY",
        litellm_prefix: "moonshot",
        skip_prefixes: &["moonshot/", "openrouter/"],
        is_gateway: false,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "https://api.moonshot.ai/v1",
        strip_model_prefix: false,
        env_extras: &[EnvExtra {
            key: "MOONSHOT_API_BASE",
            value_template: "{api_base}",
        }],
        model_overrides: &[ModelOverride {
            pattern: "kimi-k2.5",
            temperature: Some(1.0),
        }],
    },
    ProviderSpec {
        name: "minimax",
        keywords: &["minimax"],
        env_key: "MINIMAX_API_KEY",
        litellm_prefix: "minimax",
        skip_prefixes: &["minimax/", "openrouter/"],
        is_gateway: false,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "https://api.minimax.io/v1",
        strip_model_prefix: false,
        env_extras: &[],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "vllm",
        keywords: &["vllm"],
        env_key: "HOSTED_VLLM_API_KEY",
        litellm_prefix: "hosted_vllm",
        skip_prefixes: &[],
        is_gateway: false,
        is_local: true,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "",
        strip_model_prefix: false,
        env_extras: &[],
        model_overrides: &[],
    },
    ProviderSpec {
        name: "groq",
        keywords: &["groq"],
        env_key: "GROQ_API_KEY",
        litellm_prefix: "groq",
        skip_prefixes: &["groq/"],
        is_gateway: false,
        is_local: false,
        detect_by_key_prefix: "",
        detect_by_base_keyword: "",
        default_api_base: "",
        strip_model_prefix: false,
        env_extras: &[],
        model_overrides: &[],
    },
];

fn find_by_name(name: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|spec| spec.name == name)
}

fn find_by_model(model: &str) -> Option<&'static ProviderSpec> {
    let model_lower = model.to_lowercase();
    PROVIDERS.iter().find(|spec| {
        !spec.is_gateway
            && !spec.is_local
            && spec.keywords.iter().any(|kw| model_lower.contains(kw))
    })
}

fn find_gateway(
    provider_name: Option<&str>,
    api_key: Option<&str>,
    api_base: Option<&str>,
) -> Option<&'static ProviderSpec> {
    if let Some(name) = provider_name
        && let Some(spec) = find_by_name(name)
        && (spec.is_gateway || spec.is_local)
    {
        return Some(spec);
    }

    PROVIDERS.iter().find(|spec| {
        let key_matches = !spec.detect_by_key_prefix.is_empty()
            && api_key.is_some_and(|k| k.starts_with(spec.detect_by_key_prefix));
        let base_matches = !spec.detect_by_base_keyword.is_empty()
            && api_base.is_some_and(|b| b.contains(spec.detect_by_base_keyword));
        key_matches || base_matches
    })
}

#[derive(Clone)]
pub struct LiteLLMProvider {
    api_key: String,
    api_base: Option<String>,
    default_model: String,
    extra_headers: HashMap<String, String>,
    gateway: Option<&'static ProviderSpec>,
}

impl LiteLLMProvider {
    pub fn new(
        api_key: impl Into<String>,
        api_base: Option<String>,
        default_model: impl Into<String>,
        extra_headers: Option<HashMap<String, String>>,
        provider_name: Option<&str>,
    ) -> Self {
        let api_key = api_key.into();
        let default_model = default_model.into();
        let gateway = find_gateway(
            provider_name,
            if api_key.is_empty() {
                None
            } else {
                Some(&api_key)
            },
            api_base.as_deref(),
        );

        let provider = Self {
            api_key,
            api_base,
            default_model,
            extra_headers: extra_headers.unwrap_or_default(),
            gateway,
        };

        if !provider.api_key.is_empty() {
            provider.setup_env(&provider.default_model);
        }

        provider
    }

    fn resolve_model(&self, model: &str) -> String {
        if let Some(gateway) = self.gateway {
            let normalized = if gateway.strip_model_prefix {
                model.rsplit('/').next().unwrap_or(model)
            } else {
                model
            };
            if gateway.litellm_prefix.is_empty()
                || normalized.starts_with(&format!("{}/", gateway.litellm_prefix))
            {
                return normalized.to_string();
            }
            return format!("{}/{}", gateway.litellm_prefix, normalized);
        }

        if let Some(spec) = find_by_model(model)
            && !spec.litellm_prefix.is_empty()
            && !spec
                .skip_prefixes
                .iter()
                .any(|prefix| model.starts_with(prefix))
        {
            return format!("{}/{}", spec.litellm_prefix, model);
        }

        model.to_string()
    }

    fn apply_model_overrides(&self, model: &str, temperature: &mut f32) {
        let model_lower = model.to_lowercase();
        if let Some(spec) = find_by_model(model) {
            for rule in spec.model_overrides {
                if model_lower.contains(rule.pattern)
                    && let Some(temp) = rule.temperature
                {
                    *temperature = temp;
                    return;
                }
            }
        }
    }

    fn effective_api_base(&self, model: &str) -> Option<String> {
        if let Some(base) = &self.api_base {
            return Some(base.clone());
        }

        if let Some(gateway) = self.gateway
            && !gateway.default_api_base.is_empty()
        {
            return Some(gateway.default_api_base.to_string());
        }

        if let Some(spec) = find_by_model(model)
            && !spec.default_api_base.is_empty()
        {
            return Some(spec.default_api_base.to_string());
        }

        None
    }

    fn setup_env(&self, model: &str) {
        let Some(spec) = self.gateway.or_else(|| find_by_model(model)) else {
            return;
        };

        if !spec.env_key.is_empty() {
            Self::set_env_var(spec.env_key, &self.api_key, self.gateway.is_some());
        }

        let effective_base = self.api_base.as_deref().unwrap_or(spec.default_api_base);
        for extra in spec.env_extras {
            let value = extra
                .value_template
                .replace("{api_key}", &self.api_key)
                .replace("{api_base}", effective_base);
            Self::set_env_var(extra.key, &value, false);
        }
    }

    fn set_env_var(key: &str, value: &str, overwrite: bool) {
        if key.is_empty() || value.is_empty() {
            return;
        }
        if !overwrite && std::env::var_os(key).is_some() {
            return;
        }

        // SAFETY: We only mutate process env during provider initialization,
        // mirroring Python nanobot behavior for LiteLLM provider compatibility.
        unsafe { std::env::set_var(key, value) };
    }

    fn convert_message(raw: &Value) -> Message {
        if let Ok(message) = serde_json::from_value::<Message>(raw.clone()) {
            return message;
        }

        let role = match raw.get("role").and_then(Value::as_str).unwrap_or("user") {
            "system" => MessageRole::System,
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            "function" => MessageRole::Function,
            _ => MessageRole::User,
        };

        let content = match raw.get("content") {
            Some(Value::String(text)) => Some(MessageContent::Text(text.clone())),
            Some(Value::Array(parts)) => {
                serde_json::from_value::<MessageContent>(Value::Array(parts.clone())).ok()
            }
            _ => None,
        };

        let mut message = Message {
            role,
            content,
            ..Default::default()
        };

        if let Some(name) = raw.get("name").and_then(Value::as_str) {
            message.name = Some(name.to_string());
        }
        if let Some(tool_call_id) = raw.get("tool_call_id").and_then(Value::as_str) {
            message.tool_call_id = Some(tool_call_id.to_string());
        }
        if let Some(tool_calls) = raw.get("tool_calls")
            && let Ok(parsed) = serde_json::from_value(tool_calls.clone())
        {
            message.tool_calls = Some(parsed);
        }
        if let Some(function_call) = raw.get("function_call")
            && let Ok(parsed) = serde_json::from_value(function_call.clone())
        {
            message.function_call = Some(parsed);
        }

        message
    }

    fn content_to_text(content: &MessageContent) -> String {
        match content {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Parts(parts) => {
                let chunks = parts
                    .iter()
                    .filter_map(|part| match part {
                        ContentPart::Text { text } => Some(text.clone()),
                        ContentPart::ToolResult { content, .. } => Some(content.to_string()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                chunks.join("\n")
            }
        }
    }
}

#[async_trait]
impl LLMProvider for LiteLLMProvider {
    async fn chat(
        &self,
        messages: &[Value],
        tools: Option<&[Value]>,
        model: Option<&str>,
        max_tokens: u32,
        temperature: f32,
    ) -> Result<LLMResponse> {
        let selected_model = model.unwrap_or(&self.default_model);
        let resolved_model = self.resolve_model(selected_model);
        let mut effective_temperature = temperature;
        self.apply_model_overrides(&resolved_model, &mut effective_temperature);

        let chat_messages = messages
            .iter()
            .map(Self::convert_message)
            .collect::<Vec<_>>();
        let mut options = CompletionOptions {
            max_tokens: Some(max_tokens),
            temperature: Some(effective_temperature),
            api_key: if self.api_key.is_empty() {
                None
            } else {
                Some(self.api_key.clone())
            },
            api_base: self.effective_api_base(selected_model),
            headers: if self.extra_headers.is_empty() {
                None
            } else {
                Some(self.extra_headers.clone())
            },
            ..Default::default()
        };

        if let Some(tool_defs) = tools {
            let parsed_tools = tool_defs
                .iter()
                .filter_map(|item| serde_json::from_value::<Tool>(item.clone()).ok())
                .collect::<Vec<_>>();
            if !parsed_tools.is_empty() {
                options.tools = Some(parsed_tools);
                options.tool_choice = Some(ToolChoice::String("auto".to_string()));
            }

            // litellm-rs 0.3.1 conversion currently drops CompletionOptions.tools.
            options
                .extra_params
                .insert("tools".to_string(), Value::Array(tool_defs.to_vec()));
            options
                .extra_params
                .insert("tool_choice".to_string(), Value::String("auto".to_string()));
        }

        let response = match completion(
            &resolved_model,
            chat_messages.clone(),
            Some(options.clone()),
        )
        .await
        {
            Ok(resp) => resp,
            Err(primary_err) => {
                // Fallback to raw model for better compatibility with pure
                // OpenAI-compatible endpoints.
                if resolved_model != selected_model {
                    completion(selected_model, chat_messages, Some(options))
                        .await
                        .map_err(|fallback_err| {
                            anyhow::anyhow!(
                                "failed to call litellm-rs completion: primary={primary_err}; fallback={fallback_err}"
                            )
                        })?
                } else {
                    return Err(anyhow::anyhow!(
                        "failed to call litellm-rs completion: {primary_err}"
                    ));
                }
            }
        };

        let Some(choice) = response.choices.first() else {
            return Ok(LLMResponse {
                content: None,
                tool_calls: Vec::new(),
                finish_reason: "stop".to_string(),
                usage: Map::new(),
                reasoning_content: None,
            });
        };

        let content = choice.message.content.as_ref().map(Self::content_to_text);
        let reasoning_content = choice
            .message
            .thinking
            .as_ref()
            .and_then(|thinking| thinking.as_text())
            .map(ToOwned::to_owned);
        let tool_calls = choice
            .message
            .tool_calls
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|call| {
                let arguments = serde_json::from_str::<Value>(&call.function.arguments)
                    .ok()
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_else(|| {
                        let mut fallback = Map::new();
                        fallback.insert("raw".to_string(), Value::String(call.function.arguments));
                        fallback
                    });

                ToolCallRequest {
                    id: call.id,
                    name: call.function.name,
                    arguments,
                }
            })
            .collect::<Vec<_>>();

        let finish_reason = choice
            .finish_reason
            .as_ref()
            .and_then(|reason| serde_json::to_value(reason).ok())
            .and_then(|v| v.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "stop".to_string());

        let usage = response
            .usage
            .and_then(|usage| serde_json::to_value(usage).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();

        Ok(LLMResponse {
            content,
            tool_calls,
            finish_reason,
            usage,
            reasoning_content,
        })
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_detects_by_provider_name_and_key_prefix() {
        let by_name = find_gateway(Some("vllm"), None, None).expect("expected vllm gateway");
        assert_eq!(by_name.name, "vllm");

        let by_key = find_gateway(None, Some("sk-or-test"), None).expect("expected openrouter");
        assert_eq!(by_key.name, "openrouter");
    }

    #[test]
    fn resolve_model_applies_gateway_and_provider_rules() {
        let aihubmix = LiteLLMProvider::new(
            "",
            Some("https://aihubmix.com/v1".to_string()),
            "anthropic/claude-3-7-sonnet",
            None,
            Some("aihubmix"),
        );
        assert_eq!(
            aihubmix.resolve_model("anthropic/claude-3-7-sonnet"),
            "openai/claude-3-7-sonnet"
        );

        let standard = LiteLLMProvider::new("", None, "qwen-plus", None, None);
        assert_eq!(standard.resolve_model("qwen-plus"), "dashscope/qwen-plus");
        assert_eq!(
            standard.resolve_model("dashscope/qwen-plus"),
            "dashscope/qwen-plus"
        );
    }

    #[test]
    fn model_override_applies_kimi_temperature_floor() {
        let provider = LiteLLMProvider::new("", None, "kimi-k2.5", None, None);
        let mut temp = 0.2;
        provider.apply_model_overrides("moonshot/kimi-k2.5", &mut temp);
        assert!((temp - 1.0).abs() < f32::EPSILON);
    }
}
