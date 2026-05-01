mod jobs;

use std::{collections::HashMap, time::Duration};

use tokio::{sync::mpsc::Receiver, task::JoinHandle};
use tower_lsp::{Client, lsp_types::TextEdit};
use triomphe::Arc;

use crate::GlobalState;

pub struct Worker {
    client: Client,
    state: Arc<GlobalState>,
    rx: Receiver<Job>,

    tasks: HashMap<TaskKey, JoinHandle<()>>,
}

impl Worker {
    pub fn new(client: Client, state: Arc<GlobalState>, rx: Receiver<Job>) -> Self {
        Self {
            client,
            state,
            rx,
            tasks: HashMap::default(),
        }
    }

    pub fn spawn_in_background(self) {
        tokio::spawn(self.run());
    }

    async fn run(mut self) {
        while let Some(job) = self.rx.recv().await {
            let key = job.key.clone();

            let state = self.state.clone();
            let client = self.client.clone();
            let action = job.action;
            let delay = job.delay;

            let task_handle = tokio::spawn(async move {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }

                action.execute(state, client).await;
            });

            if let Some(old_task) = self.tasks.insert(key, task_handle) {
                old_task.abort();
            }

            self.tasks.retain(|_, task| !task.is_finished());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TaskKey {
    File(vfs::FileId),
    ExternalFile(String),
    ConfigReload,
    WorkspaceIndex,
}

pub struct Job {
    pub key: TaskKey,
    pub delay: Duration,
    pub action: Action,
}

impl Job {
    pub fn new(key: TaskKey, delay: Duration, action: Action) -> Self {
        Self { key, delay, action }
    }

    pub fn file(file_id: vfs::FileId, delay: Duration, action: Action) -> Self {
        Self::new(TaskKey::File(file_id), delay, action)
    }
}

pub enum Action {
    IncrementalParse {
        file_id: vfs::FileId,
        text_edit: TextEdit,
    },
    FullParse(vfs::FileId),
}

impl Action {
    pub async fn execute(self, state: Arc<GlobalState>, _client: Client) {
        match self {
            Action::IncrementalParse { file_id, text_edit } => {
                jobs::incremental_parse(state, file_id, text_edit).await;
            }

            Action::FullParse(file_id) => {
                jobs::full_parse(state, file_id).await;
            }
        }
    }
}
