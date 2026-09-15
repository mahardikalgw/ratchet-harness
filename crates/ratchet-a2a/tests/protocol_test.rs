use ratchet_a2a::{
    AgentCard, Message, Part, Task, TaskSendParams, TaskState, TaskStatus,
};

#[test]
fn card_advertises_the_spec_driven_skills() {
    let card = AgentCard::ratchet("http://127.0.0.1:8788/a2a", "0.1.0");
    assert_eq!(card.name, "Ratchet");
    assert_eq!(card.url, "http://127.0.0.1:8788/a2a");

    let ids: Vec<_> = card.skills.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"run-spec"));
    assert!(ids.contains(&"plan-spec"));
    assert!(ids.contains(&"verify-spec"));
}

#[test]
fn card_round_trips_through_json() {
    let card = AgentCard::ratchet("http://x/a2a", "1");
    let json = serde_json::to_string(&card).unwrap();
    let back: AgentCard = serde_json::from_str(&json).unwrap();
    assert_eq!(card, back);
}

#[test]
fn task_transitions_are_recorded_in_history() {
    let mut task = Task::new("t-1");
    assert_eq!(task.status.state, TaskState::Submitted);

    task.transition(TaskStatus::with_message(TaskState::Working, "starting"));
    task.transition(TaskStatus::with_message(TaskState::Completed, "done"));

    assert_eq!(task.status.state, TaskState::Completed);
    assert_eq!(task.history.len(), 2);
    assert!(task.status.terminal());
}

#[test]
fn terminal_states_are_recognised() {
    for state in [TaskState::Completed, TaskState::Canceled, TaskState::Failed] {
        assert!(TaskStatus::new(state).terminal());
    }
    for state in [TaskState::Submitted, TaskState::Working, TaskState::InputRequired, TaskState::Unknown] {
        assert!(!TaskStatus::new(state).terminal());
    }
}

#[test]
fn artifacts_carry_text_parts() {
    let mut task = Task::new("t-1");
    task.add_text_artifact("verification", "all criteria passed");

    assert_eq!(task.artifacts.len(), 1);
    match &task.artifacts[0].parts[0] {
        Part::Text { text } => assert_eq!(text, "all criteria passed"),
        other => panic!("expected text part, got {other:?}"),
    }
}

#[test]
fn send_params_read_spec_id_from_metadata() {
    let params = TaskSendParams {
        id: None,
        session_id: None,
        message: Message::user_text("do the thing"),
        metadata: serde_json::json!({"spec_id": "slugify"}),
    };
    assert_eq!(params.spec_id().as_deref(), Some("slugify"));
}

#[test]
fn send_params_fall_back_to_a_spec_token_in_text() {
    let params = TaskSendParams {
        id: None,
        session_id: None,
        message: Message::user_text("please run spec:billing-reminders now"),
        metadata: serde_json::Value::Null,
    };
    assert_eq!(params.spec_id().as_deref(), Some("billing-reminders"));
}

#[test]
fn send_params_without_a_spec_reference_are_rejected_by_caller() {
    let params = TaskSendParams {
        id: None,
        session_id: None,
        message: Message::user_text("hello there"),
        metadata: serde_json::Value::Null,
    };
    assert_eq!(params.spec_id(), None);
}

#[test]
fn send_params_read_the_requested_skill() {
    let params = TaskSendParams {
        id: None,
        session_id: None,
        message: Message::user_text("x"),
        metadata: serde_json::json!({"skill": "verify-spec"}),
    };
    assert_eq!(params.skill().as_deref(), Some("verify-spec"));
}

#[test]
fn message_text_joins_multiple_parts() {
    let message = Message {
        role: "user".to_string(),
        parts: vec![
            Part::Text { text: "line one".to_string() },
            Part::Text { text: "line two".to_string() },
        ],
    };
    assert_eq!(message.text(), "line one\nline two");
}

#[test]
fn task_round_trips_through_json() {
    let mut task = Task::new("t-1");
    task.transition(TaskStatus::with_message(TaskState::Working, "busy"));
    task.add_text_artifact("summary", "worked");

    let json = serde_json::to_string(&task).unwrap();
    let back: Task = serde_json::from_str(&json).unwrap();
    assert_eq!(task, back);
}
