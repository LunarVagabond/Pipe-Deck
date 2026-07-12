use crate::config::store::ConfigStore;
use crate::core::models::{RoutingRulesConfig, Rule, RuleCondition};
use crate::pipewire::adapter::AdapterError;

pub fn migrate_routing_rules_to_authored(rules: &RoutingRulesConfig) -> Vec<Rule> {
    rules
        .stream_rules
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            let mut conditions = Vec::new();
            if let Some(app_name) = &rule.app_name {
                conditions.push(RuleCondition::AppName {
                    value: app_name.clone(),
                });
            }
            if let Some(executable) = &rule.executable {
                conditions.push(RuleCondition::Executable {
                    value: executable.clone(),
                });
            }
            if let Some(media_name) = &rule.media_name {
                conditions.push(RuleCondition::MediaName {
                    value: media_name.clone(),
                });
            }

            Rule {
                id: format!("migrated-stream-{index}"),
                name: format!(
                    "Migrated: {}",
                    rule.app_name.as_deref().unwrap_or_else(|| {
                        rule.executable.as_deref().unwrap_or("stream")
                    })
                ),
                enabled: true,
                priority: -1_000 - index as i32,
                conditions,
                action: crate::core::models::RuleAction {
                    target_system_name: rule.target_system_name.clone(),
                    target_system_names: rule.target_system_names_resolved(),
                },
                safeguards: Default::default(),
            }
        })
        .collect()
}

pub fn ensure_rules_migrated() -> Result<(), AdapterError> {
    let store = ConfigStore::new();
    let mut config = store
        .load_config()
        .map_err(|error| AdapterError::Message(error.to_string()))?;

    if !config.rules.is_empty() || config.routing_rules.stream_rules.is_empty() {
        return Ok(());
    }

    config.rules = migrate_routing_rules_to_authored(&config.routing_rules);
    config.routing_rules.stream_rules.clear();
    store
        .save_config(&config)
        .map_err(|error| AdapterError::Message(error.to_string()))
}
