use async_trait::async_trait;
use clawseed_tools::ask_user::{
    ASK_USER_TIMEOUT, AskUserHandler, AskUserRequest, AskUserResponse, AskUserStatus,
    validate_response,
};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::{broadcast, oneshot};

struct PendingRequest {
    request: AskUserRequest,
    response_tx: oneshot::Sender<AskUserResponse>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SubmitError {
    NotPending,
    IdentityMismatch,
    InvalidResponse(String),
}

pub struct AskUserManager {
    pending: Mutex<HashMap<String, PendingRequest>>,
    event_tx: broadcast::Sender<serde_json::Value>,
}

impl AskUserManager {
    pub fn new(event_tx: broadcast::Sender<serde_json::Value>) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            event_tx,
        }
    }

    pub fn pending_for_session(&self, session_id: &str) -> Vec<AskUserRequest> {
        self.pending
            .lock()
            .expect("ask_user pending lock poisoned")
            .values()
            .filter(|pending| pending.request.session_id == session_id)
            .map(|pending| pending.request.clone())
            .collect()
    }

    pub fn submit(
        &self,
        session_id: &str,
        request_id: &str,
        turn_id: &str,
        tool_call_id: &str,
        response: AskUserResponse,
    ) -> Result<(), SubmitError> {
        let mut pending = self.pending.lock().expect("ask_user pending lock poisoned");
        let Some(request) = pending.get(request_id).map(|pending| &pending.request) else {
            return Err(SubmitError::NotPending);
        };
        if request.session_id != session_id
            || request.turn_id != turn_id
            || request.tool_call_id != tool_call_id
        {
            return Err(SubmitError::IdentityMismatch);
        }
        validate_response(&request.input, &response).map_err(SubmitError::InvalidResponse)?;
        let pending = pending
            .remove(request_id)
            .expect("pending request disappeared");
        let _ = pending.response_tx.send(response);
        Ok(())
    }

    pub fn cancel_turn(&self, session_id: &str, turn_id: &str) {
        let mut pending = self.pending.lock().expect("ask_user pending lock poisoned");
        let ids = pending
            .iter()
            .filter(|(_, pending)| {
                pending.request.session_id == session_id && pending.request.turn_id == turn_id
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            if let Some(pending) = pending.remove(&id) {
                let _ = pending.response_tx.send(AskUserResponse {
                    status: AskUserStatus::Cancelled,
                    answer: None,
                });
            }
        }
    }

    fn remove_if_pending(&self, request_id: &str) {
        self.pending
            .lock()
            .expect("ask_user pending lock poisoned")
            .remove(request_id);
    }
}

#[async_trait]
impl AskUserHandler for AskUserManager {
    async fn ask(&self, request: AskUserRequest) -> Result<AskUserResponse, String> {
        let (response_tx, response_rx) = oneshot::channel();
        self.pending
            .lock()
            .expect("ask_user pending lock poisoned")
            .insert(
                request.request_id.clone(),
                PendingRequest {
                    request: request.clone(),
                    response_tx,
                },
            );
        let mut event = serde_json::to_value(&request).map_err(|error| error.to_string())?;
        if let Some(object) = event.as_object_mut() {
            if let Some(kind) = object.get("type").cloned() {
                object.insert("kind".into(), kind);
            }
            object.insert("type".into(), serde_json::json!("ask_user_request"));
        }
        let _ = self.event_tx.send(event);

        match tokio::time::timeout(
            ASK_USER_TIMEOUT - std::time::Duration::from_secs(1),
            response_rx,
        )
        .await
        {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Ok(AskUserResponse {
                status: AskUserStatus::Cancelled,
                answer: None,
            }),
            Err(_) => {
                self.remove_if_pending(&request.request_id);
                Ok(AskUserResponse {
                    status: AskUserStatus::Expired,
                    answer: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_tools::ask_user::{AskUserInput, AskUserKind};
    use serde_json::json;
    use std::sync::Arc;

    fn request(session: &str) -> AskUserRequest {
        AskUserRequest {
            request_id: "request-1".into(),
            session_id: session.into(),
            turn_id: "turn-1".into(),
            tool_call_id: "call-1".into(),
            input: AskUserInput {
                kind: AskUserKind::Text,
                question: "What name?".into(),
                options: vec![],
                placeholder: None,
            },
        }
    }

    #[tokio::test]
    async fn accepts_only_first_matching_response() {
        let (event_tx, _) = broadcast::channel(8);
        let manager = Arc::new(AskUserManager::new(event_tx));
        let task = {
            let manager = manager.clone();
            tokio::spawn(async move { manager.ask(request("session-a")).await.unwrap() })
        };
        tokio::task::yield_now().await;
        let response = AskUserResponse {
            status: AskUserStatus::Accepted,
            answer: Some(json!("Ada")),
        };
        assert_eq!(
            manager.submit(
                "session-b",
                "request-1",
                "turn-1",
                "call-1",
                response.clone()
            ),
            Err(SubmitError::IdentityMismatch)
        );
        assert_eq!(
            manager.submit(
                "session-a",
                "request-1",
                "turn-1",
                "call-1",
                response.clone()
            ),
            Ok(())
        );
        assert_eq!(
            manager.submit("session-a", "request-1", "turn-1", "call-1", response),
            Err(SubmitError::NotPending)
        );
        assert_eq!(task.await.unwrap().status, AskUserStatus::Accepted);
    }

    #[tokio::test]
    async fn cancellation_finishes_pending_request() {
        let (event_tx, _) = broadcast::channel(8);
        let manager = Arc::new(AskUserManager::new(event_tx));
        let task = {
            let manager = manager.clone();
            tokio::spawn(async move { manager.ask(request("session-a")).await.unwrap() })
        };
        tokio::task::yield_now().await;
        manager.cancel_turn("session-a", "turn-1");
        assert_eq!(task.await.unwrap().status, AskUserStatus::Cancelled);
        assert!(manager.pending_for_session("session-a").is_empty());
    }
}
