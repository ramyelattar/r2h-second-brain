use std::sync::Arc;

use knowledge_app::{AppConfig, AppError, KnowledgeCore};

#[derive(Clone)]
pub struct AppState {
    knowledge_core: Arc<KnowledgeCore>,
}

impl AppState {
    pub fn open(config: AppConfig) -> Result<Self, AppError> {
        Ok(Self {
            knowledge_core: Arc::new(KnowledgeCore::open(config)?),
        })
    }

    pub fn schema_version(&self) -> Result<u32, AppError> {
        self.knowledge_core.schema_version()
    }

    pub(crate) fn core(&self) -> &KnowledgeCore {
        &self.knowledge_core
    }
}
