mod ecs;
mod lambda;

pub use ecs::{
    EcsClusterConfig, EcsCurrentState, EcsNetworkConfig, EcsProvisioningConfig, EcsServiceConfig,
    EcsTaskConfig,
};
pub use lambda::{
    compute_lambda_diff, LambdaCurrentState, LambdaProvisioningConfig, LambdaStateDiff,
    LambdaVpcConfig,
};

use serde::{Deserialize, Serialize};

/// Result of a provisioning operation, showing what changed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningResult {
    pub resource_type: String,
    pub resource_name: String,
    pub action: ProvisioningAction,
    pub details: Vec<String>,
}

impl ProvisioningResult {
    pub fn created(resource_type: &str, resource_name: &str, details: Vec<String>) -> Self {
        Self {
            resource_type: resource_type.to_string(),
            resource_name: resource_name.to_string(),
            action: ProvisioningAction::Created,
            details,
        }
    }

    pub fn updated(resource_type: &str, resource_name: &str, details: Vec<String>) -> Self {
        Self {
            resource_type: resource_type.to_string(),
            resource_name: resource_name.to_string(),
            action: ProvisioningAction::Updated,
            details,
        }
    }

    pub fn unchanged(resource_type: &str, resource_name: &str) -> Self {
        Self {
            resource_type: resource_type.to_string(),
            resource_name: resource_name.to_string(),
            action: ProvisioningAction::Unchanged,
            details: vec![],
        }
    }
}

/// The action taken during provisioning
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvisioningAction {
    Created,
    Updated,
    Unchanged,
}

/// Planned changes for a provisioning operation (dry-run result)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisioningPlan {
    pub changes: Vec<PlannedChange>,
}

impl ProvisioningPlan {
    pub fn new() -> Self {
        Self { changes: vec![] }
    }

    pub fn add(&mut self, change: PlannedChange) {
        self.changes.push(change);
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn has_creates(&self) -> bool {
        self.changes
            .iter()
            .any(|c| c.action == PlannedAction::Create)
    }

    pub fn has_updates(&self) -> bool {
        self.changes
            .iter()
            .any(|c| c.action == PlannedAction::Update)
    }
}

impl Default for ProvisioningPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// A single planned change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedChange {
    pub resource_type: String,
    pub resource_name: String,
    pub action: PlannedAction,
    pub current: Option<String>,
    pub desired: Option<String>,
    pub reason: String,
}

impl PlannedChange {
    pub fn create(resource_type: &str, resource_name: &str, reason: &str) -> Self {
        Self {
            resource_type: resource_type.to_string(),
            resource_name: resource_name.to_string(),
            action: PlannedAction::Create,
            current: None,
            desired: None,
            reason: reason.to_string(),
        }
    }

    pub fn update(
        resource_type: &str,
        resource_name: &str,
        current: &str,
        desired: &str,
        reason: &str,
    ) -> Self {
        Self {
            resource_type: resource_type.to_string(),
            resource_name: resource_name.to_string(),
            action: PlannedAction::Update,
            current: Some(current.to_string()),
            desired: Some(desired.to_string()),
            reason: reason.to_string(),
        }
    }

    pub fn no_change(resource_type: &str, resource_name: &str) -> Self {
        Self {
            resource_type: resource_type.to_string(),
            resource_name: resource_name.to_string(),
            action: PlannedAction::NoChange,
            current: None,
            desired: None,
            reason: "No changes needed".to_string(),
        }
    }
}

/// Planned action type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlannedAction {
    Create,
    Update,
    NoChange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provisioning_result_created() {
        let result =
            ProvisioningResult::created("Lambda", "my-function", vec!["Created new function".into()]);

        assert_eq!(result.resource_type, "Lambda");
        assert_eq!(result.resource_name, "my-function");
        assert_eq!(result.action, ProvisioningAction::Created);
        assert_eq!(result.details.len(), 1);
    }

    #[test]
    fn test_provisioning_plan() {
        let mut plan = ProvisioningPlan::new();
        assert!(plan.is_empty());

        plan.add(PlannedChange::create("Lambda", "my-function", "Function does not exist"));
        assert!(!plan.is_empty());
        assert!(plan.has_creates());
        assert!(!plan.has_updates());

        plan.add(PlannedChange::update(
            "Lambda",
            "other-function",
            "256",
            "512",
            "Memory size changed",
        ));
        assert!(plan.has_updates());
    }

    #[test]
    fn test_planned_change_no_change() {
        let change = PlannedChange::no_change("ECS Cluster", "my-cluster");

        assert_eq!(change.action, PlannedAction::NoChange);
        assert!(change.current.is_none());
        assert!(change.desired.is_none());
    }
}
