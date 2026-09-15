use ratchet_spec::TaskId;
use serde::{Deserialize, Serialize};

/// Explicit state machine for the agent loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentState {
    Idle,
    Planning,
    Executing,
    Verifying,
    Blocked(BlockedReason),
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockedReason {
    NeedsApproval { action: String, details: String },
    NeedsHumanInput { prompt: String },
    ProviderUnavailable { provider: String },
    VerificationFailed { task_id: TaskId, details: String },
}

/// State machine transitions.
pub struct StateMachine {
    state: AgentState,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            state: AgentState::Idle,
        }
    }

    pub fn state(&self) -> AgentState {
        self.state.clone()
    }

    pub fn transition(&mut self, new_state: AgentState) -> Result<(), String> {
        let valid = matches!(
            (&self.state, &new_state),
            (AgentState::Idle, AgentState::Planning)
                | (AgentState::Idle, AgentState::Executing)
                | (AgentState::Planning, AgentState::Executing)
                | (AgentState::Planning, AgentState::Done)
                | (AgentState::Planning, AgentState::Failed)
                | (AgentState::Executing, AgentState::Verifying)
                | (AgentState::Executing, AgentState::Blocked(_))
                | (AgentState::Executing, AgentState::Failed)
                | (AgentState::Verifying, AgentState::Done)
                | (AgentState::Verifying, AgentState::Blocked(_))
                | (AgentState::Verifying, AgentState::Failed)
                | (AgentState::Verifying, AgentState::Executing)
                | (AgentState::Blocked(_), AgentState::Executing)
                | (AgentState::Blocked(_), AgentState::Done)
                | (AgentState::Blocked(_), AgentState::Failed)
                | (AgentState::Done, AgentState::Idle)
                | (AgentState::Failed, AgentState::Idle)
                | (AgentState::Failed, AgentState::Planning)
                | (_, AgentState::Failed)
        );

        if valid {
            self.state = new_state;
            Ok(())
        } else {
            Err(format!(
                "invalid state transition: {:?} -> {:?}",
                self.state, new_state
            ))
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.state, AgentState::Done | AgentState::Failed)
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}
