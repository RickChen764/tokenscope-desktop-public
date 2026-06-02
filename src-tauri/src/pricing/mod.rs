use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::usage::NormalizedUsage;

pub const DEFAULT_COST_CURRENCY: &str = "USD";

fn default_cost_currency() -> String {
    DEFAULT_COST_CURRENCY.to_string()
}

pub fn normalize_currency(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

pub fn is_supported_currency(value: &str) -> bool {
    matches!(normalize_currency(value).as_str(), "USD" | "CNY")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct PricingRule {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub currency: String,
    pub input_usd_per_1m: f64,
    pub cached_input_usd_per_1m: f64,
    pub output_usd_per_1m: f64,
    pub reasoning_output_usd_per_1m: Option<f64>,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingRuleInput {
    pub id: Option<String>,
    pub provider: String,
    pub model: String,
    #[serde(default = "default_cost_currency")]
    pub currency: String,
    pub input_usd_per_1m: f64,
    pub cached_input_usd_per_1m: f64,
    pub output_usd_per_1m: f64,
    pub reasoning_output_usd_per_1m: Option<f64>,
    pub effective_from: String,
    pub effective_to: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostRecalculationResult {
    pub updated: i64,
    pub missing: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingRulePreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub source_url: Option<String>,
    pub checked_at: Option<String>,
    pub pricing_scope: Option<String>,
    pub rules: Vec<PricingRuleInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingRulePresetSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub source_url: Option<String>,
    pub checked_at: Option<String>,
    pub pricing_scope: Option<String>,
    pub rule_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricingRuleImportResult {
    pub imported: i64,
    pub updated: i64,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostEstimate {
    pub estimated_cost_usd: f64,
    pub cost_currency: String,
    pub cost_source: String,
}

pub fn builtin_pricing_presets() -> Result<Vec<PricingRulePreset>, serde_json::Error> {
    serde_json::from_str(include_str!("../../pricing_presets.json"))
}

pub fn builtin_pricing_preset_summaries() -> Result<Vec<PricingRulePresetSummary>, serde_json::Error>
{
    Ok(builtin_pricing_presets()?
        .into_iter()
        .map(|preset| PricingRulePresetSummary {
            id: preset.id,
            name: preset.name,
            description: preset.description,
            source: preset.source,
            source_url: preset.source_url,
            checked_at: preset.checked_at,
            pricing_scope: preset.pricing_scope,
            rule_count: preset.rules.len() as i64,
        })
        .collect())
}

pub fn builtin_pricing_preset(id: &str) -> Result<Option<PricingRulePreset>, serde_json::Error> {
    Ok(builtin_pricing_presets()?
        .into_iter()
        .find(|preset| preset.id == id))
}

pub fn parse_pricing_rules_json(value: &str) -> Result<Vec<PricingRuleInput>, String> {
    let parsed: Value = serde_json::from_str(value).map_err(|err| err.to_string())?;
    let rules_value = match parsed {
        Value::Array(_) => parsed,
        Value::Object(mut object) => object
            .remove("rules")
            .ok_or_else(|| "JSON object must contain a rules array".to_string())?,
        _ => return Err("pricing rules JSON must be an array or object with rules".to_string()),
    };
    let mut rules: Vec<PricingRuleInput> =
        serde_json::from_value(rules_value).map_err(|err| err.to_string())?;

    for rule in &mut rules {
        if rule.id.as_deref().map(str::trim).unwrap_or("").is_empty() {
            rule.id = Some(stable_import_rule_id(rule));
        }
        rule.currency = normalize_currency(&rule.currency);
        if rule.currency.is_empty() {
            rule.currency = DEFAULT_COST_CURRENCY.to_string();
        }
        if rule
            .source
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            rule.source = Some("json_import".to_string());
        }
    }

    Ok(rules)
}

fn stable_import_rule_id(rule: &PricingRuleInput) -> String {
    format!(
        "imported-{}-{}-{}",
        slug(&rule.provider),
        slug(&rule.model),
        slug(&rule.effective_from)
    )
}

fn slug(value: &str) -> String {
    let mut output = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
        } else if !output.ends_with('-') {
            output.push('-');
        }
    }

    output.trim_matches('-').to_string()
}

pub fn estimate_cost(usage: &NormalizedUsage, rule: Option<&PricingRule>) -> CostEstimate {
    let Some(rule) = rule else {
        return CostEstimate {
            estimated_cost_usd: 0.0,
            cost_currency: DEFAULT_COST_CURRENCY.to_string(),
            cost_source: "missing_pricing_rule".to_string(),
        };
    };

    let billable_input_tokens = (usage.input_tokens - usage.cached_input_tokens).max(0);
    let estimated_cost_usd = billable_input_tokens as f64 / 1_000_000.0 * rule.input_usd_per_1m
        + usage.cached_input_tokens as f64 / 1_000_000.0 * rule.cached_input_usd_per_1m
        + usage.output_tokens as f64 / 1_000_000.0 * rule.output_usd_per_1m;

    CostEstimate {
        estimated_cost_usd,
        cost_currency: normalize_currency(&rule.currency),
        cost_source: "pricing_rule".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::usage::{NormalizedUsage, UsageSource};

    use super::{estimate_cost, PricingRule};

    fn usage_with_cached_tokens() -> NormalizedUsage {
        NormalizedUsage {
            input_tokens: 1200,
            output_tokens: 300,
            cached_input_tokens: 800,
            cache_write_input_tokens: 0,
            reasoning_output_tokens: 50,
            audio_input_tokens: 0,
            audio_output_tokens: 0,
            image_input_tokens: 0,
            image_output_tokens: 0,
            total_tokens: 1500,
            total_billable_tokens: 1500,
            raw_usage_json: serde_json::Value::Null,
            usage_source: UsageSource::ProviderResponse,
        }
    }

    #[test]
    fn cost_estimation_with_cached_tokens_uses_discounted_cached_rate() {
        let rule = PricingRule {
            id: "test-rule".to_string(),
            provider: "openai-compatible".to_string(),
            model: "gpt-5-mini".to_string(),
            currency: "USD".to_string(),
            input_usd_per_1m: 0.25,
            cached_input_usd_per_1m: 0.025,
            output_usd_per_1m: 2.0,
            reasoning_output_usd_per_1m: None,
            effective_from: "2026-01-01".to_string(),
            effective_to: None,
            source: Some("test".to_string()),
        };

        let estimate = estimate_cost(&usage_with_cached_tokens(), Some(&rule));

        assert_eq!(estimate.cost_source, "pricing_rule");
        assert_eq!(estimate.cost_currency, "USD");
        assert!((estimate.estimated_cost_usd - 0.00072).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_estimation_carries_pricing_rule_currency() {
        let rule = PricingRule {
            id: "test-rule-cny".to_string(),
            provider: "codex".to_string(),
            model: "gpt-5.5".to_string(),
            currency: "CNY".to_string(),
            input_usd_per_1m: 7.0,
            cached_input_usd_per_1m: 0.7,
            output_usd_per_1m: 56.0,
            reasoning_output_usd_per_1m: None,
            effective_from: "2026-01-01".to_string(),
            effective_to: None,
            source: Some("test".to_string()),
        };

        let estimate = estimate_cost(&usage_with_cached_tokens(), Some(&rule));

        assert_eq!(estimate.cost_source, "pricing_rule");
        assert_eq!(estimate.cost_currency, "CNY");
        assert!((estimate.estimated_cost_usd - 0.02016).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_estimation_missing_pricing_rule_returns_zero_and_reason() {
        let estimate = estimate_cost(&usage_with_cached_tokens(), None);

        assert_eq!(estimate.estimated_cost_usd, 0.0);
        assert_eq!(estimate.cost_currency, "USD");
        assert_eq!(estimate.cost_source, "missing_pricing_rule");
    }

    #[test]
    fn cost_estimation_does_not_double_bill_reasoning_tokens() {
        let rule = PricingRule {
            id: "test-rule".to_string(),
            provider: "openai-compatible".to_string(),
            model: "gpt-5-mini".to_string(),
            currency: "USD".to_string(),
            input_usd_per_1m: 0.25,
            cached_input_usd_per_1m: 0.025,
            output_usd_per_1m: 2.0,
            reasoning_output_usd_per_1m: Some(8.0),
            effective_from: "2026-01-01".to_string(),
            effective_to: None,
            source: Some("test".to_string()),
        };
        let mut usage = usage_with_cached_tokens();
        let baseline = estimate_cost(&usage, Some(&rule)).estimated_cost_usd;

        usage.reasoning_output_tokens = 250;
        let with_reasoning = estimate_cost(&usage, Some(&rule)).estimated_cost_usd;

        assert_eq!(with_reasoning, baseline);
    }

    #[test]
    fn bundled_pricing_presets_are_parseable_and_have_stable_ids() {
        let presets = super::builtin_pricing_presets().expect("bundled presets parse");

        assert!(!presets.is_empty());
        assert!(presets.iter().all(|preset| !preset.id.trim().is_empty()));
        assert!(presets
            .iter()
            .flat_map(|preset| preset.rules.iter())
            .all(|rule| rule.id.is_some()));
    }

    #[test]
    fn official_pricing_presets_include_source_metadata_and_codex_rates() {
        let presets = super::builtin_pricing_presets().expect("bundled presets parse");
        let codex_preset = presets
            .iter()
            .find(|preset| preset.id == "openai-official-codex-standard-2026-06-01")
            .expect("official codex preset exists");

        assert_eq!(
            codex_preset.source_url.as_deref(),
            Some("https://developers.openai.com/api/docs/pricing")
        );
        assert_eq!(codex_preset.checked_at.as_deref(), Some("2026-06-01"));
        assert!(codex_preset
            .pricing_scope
            .as_deref()
            .unwrap_or_default()
            .contains("Standard"));

        let codex_rule = codex_preset
            .rules
            .iter()
            .find(|rule| rule.provider == "codex" && rule.model == "gpt-5.3-codex")
            .expect("official gpt-5.3-codex rule exists");
        assert_eq!(codex_rule.input_usd_per_1m, 1.75);
        assert_eq!(codex_rule.cached_input_usd_per_1m, 0.175);
        assert_eq!(codex_rule.output_usd_per_1m, 14.0);
    }

    #[test]
    fn pricing_rule_json_import_accepts_array_or_wrapped_rules() {
        let array_json = r#"
        [
          {
            "provider": "codex",
            "model": "gpt-5.5",
            "input_usd_per_1m": 1.0,
            "cached_input_usd_per_1m": 0.1,
            "output_usd_per_1m": 8.0,
            "effective_from": "2026-06-01"
          }
        ]
        "#;
        let wrapped_json = r#"
        {
          "rules": [
            {
              "id": "custom-rule",
              "provider": "openai-compatible",
              "model": "gpt-5-mini",
              "input_usd_per_1m": 0.25,
              "cached_input_usd_per_1m": 0.025,
              "output_usd_per_1m": 2.0,
              "effective_from": "2026-06-01"
            }
          ]
        }
        "#;

        let array_rules = super::parse_pricing_rules_json(array_json).expect("array json parses");
        let wrapped_rules =
            super::parse_pricing_rules_json(wrapped_json).expect("wrapped json parses");

        assert_eq!(array_rules.len(), 1);
        assert!(array_rules[0].id.is_some());
        assert_eq!(array_rules[0].currency, "USD");
        assert_eq!(wrapped_rules[0].id.as_deref(), Some("custom-rule"));
    }
}
