use crate::providers::base::LLMProvider;
use serde_json::{Value, json};

pub struct TurnGuard<'a> {
    provider: &'a dyn LLMProvider,
    model: &'a str,
    tools_text: String,
    max_iterations: u32,
}

impl<'a> TurnGuard<'a> {
    pub fn new(
        provider: &'a dyn LLMProvider,
        model: &'a str,
        tools_text: String,
        max_iterations: u32,
    ) -> Self {
        Self {
            provider,
            model,
            tools_text,
            max_iterations,
        }
    }

    pub fn correction_message(&self) -> Value {
        json!({
            "role": "system",
            "content": format!(
                "Correction: tools are available in this runtime. Available tools: {}. \
        Do not claim tools are unavailable; call the appropriate tool directly.",
                self.tools_text
            )
        })
    }

    pub fn tools_available_response(&self) -> String {
        if self.tools_text == "(none)" {
            "当前运行时未注册任何工具。".to_string()
        } else {
            let items = self.tools_text.split(", ").collect::<Vec<_>>().join("\n- ");
            format!(
                "当前运行时可用工具：\n- {}\n如需执行网络访问、命令执行、文件操作或定时任务，请直接给出目标。",
                items
            )
        }
    }

    pub async fn should_retry_after_false_no_tools_claim(
        &self,
        content: Option<&str>,
        iteration: u32,
    ) -> bool {
        if iteration >= self.max_iterations || self.tools_text == "(none)" {
            return false;
        }
        let Some(text) = content else {
            return false;
        };
        self.response_claims_no_tools(text).await
    }

    async fn response_claims_no_tools(&self, content: &str) -> bool {
        if content.trim().is_empty() || self.tools_text == "(none)" {
            return false;
        }

        let messages = vec![
            json!({
                "role": "system",
                "content": "You are a strict classifier. Return ONLY one JSON object with boolean key claims_no_tools. \
            If the assistant response explicitly or implicitly claims that tools are unavailable in the current runtime, set claims_no_tools=true. \
            Otherwise false. Do not output markdown or extra text."
            }),
            json!({
                "role": "user",
                "content": format!(
                    "Runtime tools are available: {}.\nAssistant response:\n{}",
                    self.tools_text,
                    content
                )
            }),
        ];

        let response = match self
            .provider
            .chat(&messages, None, Some(self.model), 120, 0.0)
            .await
        {
            Ok(v) => v,
            Err(_) => return false,
        };

        let Some(classifier_text) = response.content else {
            return false;
        };
        let Some(value) = extract_json_object(&classifier_text) else {
            return false;
        };
        value
            .get("claims_no_tools")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

fn extract_json_object(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed)
        && value.is_object()
    {
        return Some(value);
    }

    for (start, ch) in text.char_indices() {
        if ch != '{' {
            continue;
        }

        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;

        for (offset, c) in text[start..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                    continue;
                }
                match c {
                    '\\' => escaped = true,
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }

            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        let end = start + offset + c.len_utf8();
                        let candidate = &text[start..end];
                        if let Ok(value) = serde_json::from_str::<Value>(candidate)
                            && value.is_object()
                        {
                            return Some(value);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::extract_json_object;

    #[test]
    fn extract_json_object_parses_plain_json() {
        let raw = r#"{"claims_no_tools":true}"#;
        let value = extract_json_object(raw).expect("json should parse");
        assert_eq!(value["claims_no_tools"], true);
    }

    #[test]
    fn extract_json_object_parses_embedded_json() {
        let raw = "```json\n{\"claims_no_tools\":false}\n```";
        let value = extract_json_object(raw).expect("embedded json should parse");
        assert_eq!(value["claims_no_tools"], false);
    }
}
