use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    ProviderResponse,
    Estimated,
    UnavailableStreamingMvp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub audio_input_tokens: i64,
    pub audio_output_tokens: i64,
    pub image_input_tokens: i64,
    pub image_output_tokens: i64,
    pub total_tokens: i64,
    pub total_billable_tokens: i64,
    pub raw_usage_json: Value,
    pub usage_source: UsageSource,
}

impl NormalizedUsage {
    fn zero(raw_usage_json: Value, usage_source: UsageSource) -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            cache_write_input_tokens: 0,
            reasoning_output_tokens: 0,
            audio_input_tokens: 0,
            audio_output_tokens: 0,
            image_input_tokens: 0,
            image_output_tokens: 0,
            total_tokens: 0,
            total_billable_tokens: 0,
            raw_usage_json,
            usage_source,
        }
    }

    fn from_usage_object(
        usage: &Value,
        input_tokens: i64,
        output_tokens: i64,
        cached_input_tokens: i64,
        reasoning_output_tokens: i64,
    ) -> Self {
        let total_tokens =
            number_at(usage, &["total_tokens"]).unwrap_or(input_tokens + output_tokens);

        Self {
            input_tokens,
            output_tokens,
            cached_input_tokens,
            cache_write_input_tokens: 0,
            reasoning_output_tokens,
            audio_input_tokens: 0,
            audio_output_tokens: 0,
            image_input_tokens: 0,
            image_output_tokens: 0,
            total_tokens,
            total_billable_tokens: total_tokens,
            raw_usage_json: usage.clone(),
            usage_source: UsageSource::ProviderResponse,
        }
    }
}

pub fn normalize_chat_completions_usage(response: &Value) -> NormalizedUsage {
    let Some(usage) = response.get("usage") else {
        return normalize_missing_usage(response);
    };

    NormalizedUsage::from_usage_object(
        usage,
        number_at(usage, &["prompt_tokens"]).unwrap_or(0),
        number_at(usage, &["completion_tokens"]).unwrap_or(0),
        number_at(usage, &["prompt_tokens_details", "cached_tokens"]).unwrap_or(0),
        number_at(usage, &["completion_tokens_details", "reasoning_tokens"]).unwrap_or(0),
    )
}

pub fn normalize_responses_usage(response: &Value) -> NormalizedUsage {
    let Some(usage) = response.get("usage") else {
        return normalize_missing_usage(response);
    };

    NormalizedUsage::from_usage_object(
        usage,
        number_at(usage, &["input_tokens"]).unwrap_or(0),
        number_at(usage, &["output_tokens"]).unwrap_or(0),
        number_at(usage, &["input_tokens_details", "cached_tokens"]).unwrap_or(0),
        number_at(usage, &["output_tokens_details", "reasoning_tokens"]).unwrap_or(0),
    )
}

pub fn normalize_missing_usage(_response: &Value) -> NormalizedUsage {
    NormalizedUsage::zero(Value::Null, UsageSource::Estimated)
}

fn number_at(value: &Value, path: &[&str]) -> Option<i64> {
    let mut current = value;
    for segment in path {
        current = current.get(segment)?;
    }

    current.as_i64()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        normalize_chat_completions_usage, normalize_missing_usage, normalize_responses_usage,
        UsageSource,
    };

    #[test]
    fn normalize_chat_completions_usage_maps_openai_compatible_fields() {
        let response = json!({
          "usage": {
            "prompt_tokens": 1200,
            "completion_tokens": 300,
            "total_tokens": 1500,
            "prompt_tokens_details": {
              "cached_tokens": 800
            },
            "completion_tokens_details": {
              "reasoning_tokens": 50
            }
          }
        });

        let usage = normalize_chat_completions_usage(&response);

        assert_eq!(usage.input_tokens, 1200);
        assert_eq!(usage.output_tokens, 300);
        assert_eq!(usage.cached_input_tokens, 800);
        assert_eq!(usage.reasoning_output_tokens, 50);
        assert_eq!(usage.total_tokens, 1500);
        assert_eq!(usage.total_billable_tokens, 1500);
        assert_eq!(usage.raw_usage_json["prompt_tokens"], 1200);
        assert_eq!(usage.usage_source, UsageSource::ProviderResponse);
    }

    #[test]
    fn normalize_responses_usage_maps_responses_fields() {
        let response = json!({
          "usage": {
            "input_tokens": 1200,
            "output_tokens": 300,
            "total_tokens": 1500,
            "input_tokens_details": {
              "cached_tokens": 800
            },
            "output_tokens_details": {
              "reasoning_tokens": 50
            }
          }
        });

        let usage = normalize_responses_usage(&response);

        assert_eq!(usage.input_tokens, 1200);
        assert_eq!(usage.output_tokens, 300);
        assert_eq!(usage.cached_input_tokens, 800);
        assert_eq!(usage.reasoning_output_tokens, 50);
        assert_eq!(usage.total_tokens, 1500);
        assert_eq!(usage.total_billable_tokens, 1500);
        assert_eq!(usage.raw_usage_json["input_tokens"], 1200);
        assert_eq!(usage.usage_source, UsageSource::ProviderResponse);
    }

    #[test]
    fn normalize_missing_usage_returns_zero_usage_without_error() {
        let response = json!({
          "id": "missing_usage_fixture",
          "model": "gpt-5-mini"
        });

        let usage = normalize_missing_usage(&response);

        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cached_input_tokens, 0);
        assert_eq!(usage.reasoning_output_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
        assert_eq!(usage.total_billable_tokens, 0);
        assert_eq!(usage.raw_usage_json, serde_json::Value::Null);
        assert_eq!(usage.usage_source, UsageSource::Estimated);
    }

    #[test]
    fn normalize_partial_usage_defaults_missing_details_to_zero() {
        let response = json!({
          "usage": {
            "prompt_tokens": 42
          }
        });

        let usage = normalize_chat_completions_usage(&response);

        assert_eq!(usage.input_tokens, 42);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cached_input_tokens, 0);
        assert_eq!(usage.reasoning_output_tokens, 0);
        assert_eq!(usage.total_tokens, 42);
    }
}
