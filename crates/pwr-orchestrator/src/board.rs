//! A plan the model writes and the harness holds up to it.
//!
//! Not [`crate::plan`]: that one is made before a scripted run starts and its
//! steps can carry checks. This is a short list the model keeps while it
//! works, shown back to it before every reply so that it does not have to
//! find it in a history that grows and is compacted.
//!
//! Why try it (plan W2.14, a hypothesis here): arXiv 2609.20804 measured its
//! weakest model, 30B, ending 69 % of its SWE-Bench runs without an edit and
//! 58 % while still locating the problem; with a plan kept this way its
//! success rose by 11.6 %, at a higher cost, and the stronger models gained
//! nothing in accuracy. On the product path here Ornith 1.5 9B read a whole
//! project and changed none of it, ten goals in ten.
//!
//! The harness records what the model says and shows it back. It never marks
//! a step done and never adds one: whether the work advanced is the model's
//! claim, and the checks' to confirm.

use serde_json::Value;

/// Where a step stands, by the model's own account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    InProgress,
    Completed,
}

/// The plan as last written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub steps: Vec<(String, Status)>,
}

/// What a plan that cannot be read is answered with: the thing itself.
const EXAMPLE: &str = "every step needs its text and its status, like this: {\"steps\": [{\"step\": \"fix the rounding\", \"status\": \"in_progress\"}, {\"step\": \"run the tests\", \"status\": \"pending\"}]}";

/// More steps than this is a document, not a plan to hold in view.
pub const MAX_STEPS: usize = 12;
pub const TOOL: &str = "update_plan";

impl Board {
    /// The plan in an `update_plan` call, or what is wrong with it in words
    /// the model can act on.
    pub fn parse(arguments: &Value) -> Result<Self, String> {
        let steps = arguments
            .get("steps")
            .and_then(Value::as_array)
            .ok_or("update_plan takes `steps`: a list of {\"step\": text, \"status\": pending | in_progress | completed}")?;
        if steps.is_empty() || steps.len() > MAX_STEPS {
            return Err(format!(
                "a plan has 1 to {MAX_STEPS} steps; this one has {}",
                steps.len()
            ));
        }
        let mut board = Vec::new();
        for entry in steps {
            // The names a plan's fields go by. gpt-oss 20B wrote `title` and
            // `state`, then `name` and `state`, forty-five times in forty-seven
            // replies against a refusal that named `step`: a field's name is
            // not what a plan is refused for.
            let text = ["step", "title", "name", "description", "task", "text"]
                .iter()
                .find_map(|key| entry.get(*key).and_then(Value::as_str))
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .ok_or(EXAMPLE)?;
            let status = match ["status", "state"]
                .iter()
                .find_map(|key| entry.get(*key).and_then(Value::as_str))
            {
                Some("pending" | "todo" | "not_started") => Status::Pending,
                Some("in_progress" | "in-progress" | "active" | "doing") => Status::InProgress,
                Some("completed" | "done" | "complete") => Status::Completed,
                other => {
                    return Err(format!(
                        "a step's status is pending, in_progress or completed, not {}",
                        other.unwrap_or("missing")
                    ));
                }
            };
            board.push((text.chars().take(200).collect(), status));
        }
        let working = board
            .iter()
            .filter(|(_, status)| *status == Status::InProgress)
            .count();
        let open = board.iter().any(|(_, status)| *status != Status::Completed);
        // One thing at a time: the rule the measured scaffold kept.
        if working > 1 || (open && working == 0) {
            return Err(format!(
                "exactly one step is in_progress while any is unfinished; this plan has {working}"
            ));
        }
        Ok(Self { steps: board })
    }

    /// The plan as the model is shown it before a reply.
    pub fn shown(&self) -> String {
        let mut text = String::from(
            "YOUR PLAN, as you last wrote it (update_plan when a step is finished or the plan changes):",
        );
        for (number, (step, status)) in self.steps.iter().enumerate() {
            let mark = match status {
                Status::Pending => "[ ]",
                Status::InProgress => "[>]",
                Status::Completed => "[x]",
            };
            text.push_str(&format!("\n{mark} {}. {step}", number + 1));
        }
        text
    }
}

/// What a model with no plan yet is shown in its place.
pub const NO_PLAN: &str = "YOU HAVE NO PLAN YET. Before anything else call update_plan with the few steps this task needs, the first one in_progress.";

/// The tool, as a conversation's catalogue offers it.
pub fn tool() -> pwr_domain::ToolDefinition {
    pwr_domain::ToolDefinition {
        name: TOOL.into(),
        description: "Write or rewrite your plan for this task: a few concrete steps, each \
                      pending, in_progress or completed, exactly one in_progress while any is \
                      unfinished. Call it first, and again whenever you finish a step or the plan \
                      changes. PWR shows you the current plan before each of your replies; it \
                      does not check the steps."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "step": {"type": "string", "description": "What to do, in one line."},
                            "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]},
                        },
                        "required": ["step", "status"],
                    },
                },
            },
            "required": ["steps"],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(entries: &[(&str, &str)]) -> Value {
        serde_json::json!({
            "steps": entries
                .iter()
                .map(|(step, status)| serde_json::json!({"step": step, "status": status}))
                .collect::<Vec<_>>()
        })
    }

    #[test]
    fn a_plan_is_kept_as_written_and_shown_with_where_each_step_stands() {
        let board = Board::parse(&steps(&[
            ("read the failing tests", "completed"),
            ("fix rounding in src/money.ts", "in_progress"),
            ("run the tests", "pending"),
        ]))
        .unwrap();
        assert_eq!(
            board.steps[1],
            (
                "fix rounding in src/money.ts".to_owned(),
                Status::InProgress
            )
        );
        let shown = board.shown();
        assert!(shown.starts_with("YOUR PLAN, as you last wrote it"));
        assert!(shown.contains("\n[x] 1. read the failing tests"));
        assert!(shown.contains("\n[>] 2. fix rounding in src/money.ts"));
        assert!(shown.ends_with("\n[ ] 3. run the tests"));
        // A finished plan has nothing in progress, and that is not an error.
        assert!(Board::parse(&steps(&[("done", "completed")])).is_ok());
    }

    #[test]
    fn a_plan_is_read_whatever_its_fields_are_called() {
        // What gpt-oss 20B sent, both ways.
        for plan in [
            serde_json::json!({"steps": [{"title": "Run tests to confirm failures", "state": "in_progress"}]}),
            serde_json::json!({"steps": [{"name": "Run tests", "state": "in_progress"}, {"name": "Fix", "state": "todo"}]}),
            serde_json::json!({"steps": [{"description": "Run tests", "status": "done"}]}),
        ] {
            assert!(Board::parse(&plan).is_ok(), "{plan}");
        }
    }

    #[test]
    fn a_plan_that_cannot_be_held_to_is_refused_in_words() {
        for (arguments, why) in [
            (serde_json::json!({}), "takes `steps`"),
            (steps(&[]), "1 to 12 steps"),
            (
                steps(&[("a", "pending"), ("b", "pending")]),
                "exactly one step is in_progress",
            ),
            (
                steps(&[("a", "in_progress"), ("b", "in_progress")]),
                "this plan has 2",
            ),
            (steps(&[("a", "soon")]), "not soon"),
            (steps(&[("  ", "in_progress")]), "like this: {\"steps\""),
        ] {
            assert!(Board::parse(&arguments).unwrap_err().contains(why), "{why}");
        }
        let many: Vec<(&str, &str)> = (0..13)
            .map(|n| ("s", if n == 0 { "in_progress" } else { "pending" }))
            .collect();
        assert!(
            Board::parse(&steps(&many))
                .unwrap_err()
                .contains("this one has 13")
        );
    }
}
