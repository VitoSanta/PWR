//! One vocabulary for evidence, separate from the model's completion claim.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnOutcome {
    pub terminal: TurnTerminal,
    pub delivered: bool,
    pub checks: ChecksOutcome,
    pub baseline: BaselineOutcome,
    pub acceptance: AcceptanceOutcome,
    pub confinement: Confinement,
    #[serde(default)]
    pub budget: BTreeMap<String, BudgetCounter>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TurnTerminal {
    Completed,
    Blocked,
    Interrupted,
    BudgetExhausted,
    Declined,
    #[default]
    NotRun,
    Failed {
        class: super::TerminalClass,
    },
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChecksOutcome {
    #[default]
    NotRun,
    Unavailable {
        why: String,
    },
    Passed,
    RanZeroTests,
    Failed {
        fingerprints: Vec<String>,
    },
    CouldNotRun {
        why: String,
    },
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BaselineOutcome {
    Preserved,
    Regressed {
        checks: Vec<String>,
    },
    #[default]
    NoBaseline,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AcceptanceOutcome {
    Accepted,
    #[default]
    NotDeclared,
    ContractChanged {
        what: Vec<String>,
    },
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Confinement {
    Sandboxed,
    PartiallyEnforced { what: Vec<String> },
    Unconfined,
}
impl Default for Confinement {
    fn default() -> Self {
        Self::PartiallyEnforced {
            what: vec!["command confinement has not been observed".into()],
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetCounter {
    pub spent: u64,
    pub limit: Option<u64>,
}
impl TurnOutcome {
    /// Delivery and green technical checks never substitute for acceptance.
    pub fn verified(&self) -> bool {
        matches!(self.acceptance, AcceptanceOutcome::Accepted)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_round_trips_without_promoting_delivery_to_acceptance() {
        for checks in [
            ChecksOutcome::NotRun,
            ChecksOutcome::Passed,
            ChecksOutcome::RanZeroTests,
            ChecksOutcome::Failed {
                fingerprints: vec!["test_a".into()],
            },
            ChecksOutcome::CouldNotRun {
                why: "no compiler".into(),
            },
            ChecksOutcome::Unavailable {
                why: "no checks".into(),
            },
        ] {
            let outcome = TurnOutcome {
                terminal: TurnTerminal::Completed,
                delivered: true,
                checks,
                ..Default::default()
            };
            let restored: TurnOutcome =
                serde_json::from_slice(&serde_json::to_vec(&outcome).unwrap()).unwrap();
            assert_eq!(outcome, restored);
            assert!(!restored.verified());
        }
    }
}
