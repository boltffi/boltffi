use boltffi::*;

scaffolding!();
use std::sync::Mutex;

pub use demo_multicrate_model::{
    ForeignCode, ForeignCounter, ForeignKind, ForeignLabeler, ForeignPoint, ForeignState,
    ForeignUser, kind_label, model_code_value, model_echo_code, model_echo_kind,
    model_format_with_labeler, model_kind_label, model_point_sum, model_shift_point,
    model_state_summary, model_user_summary,
};

#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct ForeignSession {
    pub id: u32,
    pub user: ForeignUser,
    pub kind: ForeignKind,
}

#[data]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct ForeignMetrics {
    pub score: f64,
    pub count: u32,
}

#[data]
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    Started { session: ForeignSession },
    Stopped,
}

pub struct SessionBook {
    sessions: Mutex<Vec<ForeignSession>>,
}

#[export]
impl SessionBook {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(Vec::new()),
        }
    }

    pub fn with_session(session: ForeignSession) -> Self {
        Self {
            sessions: Mutex::new(vec![session]),
        }
    }

    pub fn add_session(&self, session: ForeignSession) -> u32 {
        let mut sessions = self.sessions.lock().expect("session book lock poisoned");
        sessions.push(session);
        sessions.len() as u32
    }

    pub fn count(&self) -> u32 {
        self.sessions
            .lock()
            .expect("session book lock poisoned")
            .len() as u32
    }

    pub fn summarize_first(&self, fallback: ForeignKind) -> String {
        self.sessions
            .lock()
            .expect("session book lock poisoned")
            .first()
            .map(|session| summarize_session(session))
            .unwrap_or_else(|| format!("empty#{}", kind_label(fallback)))
    }

    pub fn summarize_borrowed(
        &self,
        user: &ForeignUser,
        session: &ForeignSession,
        kind: &ForeignKind,
    ) -> String {
        format!(
            "{}#{}#{}#{}#{}",
            user.name,
            user.age,
            session.id,
            kind_label(session.kind),
            kind_label(*kind)
        )
    }

    pub fn metrics_for_points(&self, points: Vec<ForeignPoint>) -> ForeignMetrics {
        ForeignMetrics {
            score: points.iter().map(|point| point.x + point.y).sum(),
            count: points.len() as u32,
        }
    }
}

#[export]
pub fn session_make(id: u32, user: ForeignUser, kind: ForeignKind) -> ForeignSession {
    ForeignSession { id, user, kind }
}

#[export]
pub fn session_summary(session: ForeignSession) -> String {
    summarize_session(&session)
}

#[export]
pub fn session_total_age(sessions: Vec<ForeignSession>) -> u32 {
    sessions.iter().map(|session| session.user.age).sum::<u32>()
}

#[export]
pub fn session_optional_user_name(session: Option<ForeignSession>) -> Option<String> {
    session.map(|session| session.user.name)
}

#[export]
pub fn session_event_summary(event: SessionEvent) -> String {
    match event {
        SessionEvent::Started { session } => format!("started:{}", summarize_session(&session)),
        SessionEvent::Stopped => "stopped".to_string(),
    }
}

#[export]
pub fn session_try_make(
    id: u32,
    user: ForeignUser,
    kind: ForeignKind,
) -> Result<ForeignSession, String> {
    if id == 0 {
        Err("session id must be positive".to_string())
    } else {
        Ok(ForeignSession { id, user, kind })
    }
}

#[export]
pub fn session_apply_labeler(
    labeler: impl ForeignLabeler,
    user: ForeignUser,
    kind: ForeignKind,
) -> String {
    labeler.label(user, kind)
}

pub fn summarize_session(session: &ForeignSession) -> String {
    format!(
        "{}#{}#{}#{}",
        session.id,
        session.user.name,
        session.user.age,
        kind_label(session.kind)
    )
}
