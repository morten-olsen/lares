use anyhow::Result;
use async_trait::async_trait;
use lares_protocol::ProposedAction;

#[async_trait]
pub trait ApprovalGate: Send + Sync {
    async fn request_approval(&self, action: &ProposedAction) -> Result<bool>;
}

#[async_trait]
pub trait QuestionGate: Send + Sync {
    async fn ask_user(&self, question: &str) -> Result<String>;
}

/// Auto-approves everything. Useful for testing.
pub struct AutoApprove;

#[async_trait]
impl ApprovalGate for AutoApprove {
    async fn request_approval(&self, _action: &ProposedAction) -> Result<bool> {
        Ok(true)
    }
}
