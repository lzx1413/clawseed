use crate::knowledge::{KnowledgeRouter, KnowledgeTarget};
use crate::user_model::InferenceOptions;
use clawseed_api::memory_traits::{ConflictMode, Memory};
use clawseed_api::provider::Provider;
use clawseed_api::user_profile::{UserContext, UserProfileStore};
use parking_lot::Mutex;
use std::sync::Arc;

const LEARNING_QUEUE_CAPACITY: usize = 16;

pub(crate) struct LearningJob {
    pub provider: Arc<dyn Provider>,
    pub memory: Arc<dyn Memory>,
    pub profile_store: Option<Arc<dyn UserProfileStore>>,
    pub user_context: Option<UserContext>,
    pub model: String,
    pub user_text: String,
    pub assistant_text: String,
    pub session_id: Option<String>,
    pub persona_id: Option<String>,
    pub memory_namespace: String,
    pub auto_save: bool,
    pub profile_inference: Option<InferenceOptions>,
    pub conflict_mode: ConflictMode,
    pub conflict_threshold: f64,
}

struct CoordinatorState {
    sender: Option<tokio::sync::mpsc::Sender<LearningJob>>,
    worker: Option<tokio::task::JoinHandle<()>>,
}

pub(crate) struct KnowledgeCoordinator {
    state: Mutex<CoordinatorState>,
}

impl KnowledgeCoordinator {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(CoordinatorState {
                sender: None,
                worker: None,
            }),
        }
    }

    /// Queue learning without delaying the user-visible response. Saturation
    /// drops the newest job and records only non-sensitive routing metadata.
    pub fn submit(&self, job: LearningJob) {
        let mut state = self.state.lock();
        if state.sender.is_none() {
            let Ok(runtime) = tokio::runtime::Handle::try_current() else {
                tracing::warn!("post-turn learning skipped: no Tokio runtime");
                return;
            };
            let (sender, mut receiver) = tokio::sync::mpsc::channel(LEARNING_QUEUE_CAPACITY);
            state.sender = Some(sender);
            state.worker = Some(runtime.spawn(async move {
                while let Some(job) = receiver.recv().await {
                    process_job(job).await;
                }
                tracing::debug!("post-turn learning queue drained");
            }));
        }
        match state
            .sender
            .as_ref()
            .expect("sender initialized")
            .try_send(job)
        {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    capacity = LEARNING_QUEUE_CAPACITY,
                    "post-turn learning queue full; newest turn dropped"
                );
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("post-turn learning queue closed; turn dropped");
            }
        }
    }

    pub async fn shutdown_and_drain(&self) {
        let worker = {
            let mut state = self.state.lock();
            state.sender.take();
            state.worker.take()
        };
        if let Some(worker) = worker
            && let Err(error) = worker.await
        {
            tracing::debug!(%error, "post-turn learning worker stopped before drain completed");
        }
    }
}

impl Drop for KnowledgeCoordinator {
    fn drop(&mut self) {
        // Dropping the final sender closes the channel. The detached worker
        // drains queued jobs before it exits; callers that need confirmation
        // can use `shutdown_and_drain` before dropping the Agent.
        self.state.get_mut().sender.take();
    }
}

async fn process_job(job: LearningJob) {
    let target = KnowledgeRouter::route_user_text(&job.user_text);
    if target == KnowledgeTarget::Profile
        && let (Some(options), Some(store), Some(context)) = (
            job.profile_inference,
            job.profile_store.as_ref(),
            job.user_context.as_ref(),
        )
        && let Err(error) = crate::user_model::infer_and_store_bounded(
            job.provider.as_ref(),
            store.as_ref(),
            context,
            &job.model,
            &job.user_text,
            options,
        )
        .await
    {
        tracing::debug!(user_id = %context.user_id, %error, "post-turn profile inference failed");
    }

    if !job.auto_save
        || target != KnowledgeTarget::Memory
        || clawseed_memory::should_skip_autosave_content(&job.user_text)
    {
        tracing::debug!(
            target = ?target,
            persona_id = job.persona_id.as_deref().unwrap_or("default"),
            namespace = %job.memory_namespace,
            "post-turn memory learning skipped"
        );
        return;
    }
    if let Err(error) = clawseed_memory::consolidation::consolidate_turn(
        job.provider.as_ref(),
        &job.model,
        job.memory.as_ref(),
        &job.user_text,
        &job.assistant_text,
        job.session_id.as_deref(),
        &job.conflict_mode,
        job.conflict_threshold,
    )
    .await
    {
        tracing::debug!(%error, "post-turn memory consolidation failed");
    }
}
