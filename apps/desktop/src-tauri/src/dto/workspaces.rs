use knowledge_domain::Workspace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkspaceRequest {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDto {
    pub id: String,
    pub name: String,
    pub created_at_utc: String,
    pub archived_at_utc: Option<String>,
}

impl From<Workspace> for WorkspaceDto {
    fn from(workspace: Workspace) -> Self {
        Self {
            id: workspace.id.to_string(),
            name: workspace.name,
            created_at_utc: workspace.created_at_utc.to_rfc3339(),
            archived_at_utc: workspace.archived_at_utc.map(|value| value.to_rfc3339()),
        }
    }
}
