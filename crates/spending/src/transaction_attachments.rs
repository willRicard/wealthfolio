//! Private transaction attachment metadata and access control.
use crate::{settings::SpendingSettingsService, SpendingError};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wealthfolio_core::{
    activities::{ActivityError, ActivityServiceTrait},
    errors::Error as CoreError,
};

pub const MAX_ATTACHMENT_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_ATTACHMENTS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionAttachment {
    pub id: String,
    pub activity_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: String,
}

#[async_trait]
pub trait TransactionAttachmentsRepository: Send + Sync {
    async fn activities_with_attachments(&self, account_id: &str) -> Result<Vec<String>>;
    async fn list(&self, activity_id: &str) -> Result<Vec<TransactionAttachment>>;
    async fn insert_attachment(&self, attachment: TransactionAttachment) -> Result<()>;
    async fn delete_attachment(&self, activity_id: &str, id: &str) -> Result<()>;
}

pub struct TransactionAttachmentsService {
    repo: Arc<dyn TransactionAttachmentsRepository>,
    activities: Arc<dyn ActivityServiceTrait + Send + Sync>,
    settings: Arc<SpendingSettingsService>,
}

impl TransactionAttachmentsService {
    pub fn new(
        repo: Arc<dyn TransactionAttachmentsRepository>,
        activities: Arc<dyn ActivityServiceTrait + Send + Sync>,
        settings: Arc<SpendingSettingsService>,
    ) -> Self {
        Self {
            repo,
            activities,
            settings,
        }
    }

    // The runtime admits the unlocked profile; the domain further restricts
    // attachment access to existing activities in opted-in spending accounts.
    pub async fn check_access(&self, activity_id: &str) -> Result<()> {
        let settings = self.settings.get().await?;
        let missing = || SpendingError::NotFound {
            entity: "Spending transaction",
            id: activity_id.into(),
        };
        if !settings.enabled {
            return Err(missing().into());
        }
        let activity = match self.activities.get_activity(activity_id) {
            Ok(activity) => activity,
            Err(CoreError::Activity(ActivityError::NotFound(_))) => return Err(missing().into()),
            Err(error) => return Err(error.into()),
        };
        if !settings.account_ids.contains(&activity.account_id) {
            return Err(SpendingError::NotFound {
                entity: "Spending transaction",
                id: activity_id.into(),
            }
            .into());
        }
        Ok(())
    }

    pub async fn activities_with_attachments(&self, account_id: &str) -> Result<Vec<String>> {
        self.repo.activities_with_attachments(account_id).await
    }

    pub async fn list(&self, activity_id: &str) -> Result<Vec<TransactionAttachment>> {
        self.check_access(activity_id).await?;
        self.repo.list(activity_id).await
    }

    pub async fn add_attachment(&self, attachment: TransactionAttachment) -> Result<()> {
        self.check_access(&attachment.activity_id).await?;
        if self.repo.list(&attachment.activity_id).await?.len() >= MAX_ATTACHMENTS {
            return Err(SpendingError::InvalidInput {
                message: "Maximum 10 attachments per transaction".into(),
            }
            .into());
        }
        self.repo.insert_attachment(attachment).await
    }

    pub async fn attachment(&self, activity_id: &str, id: &str) -> Result<TransactionAttachment> {
        self.list(activity_id)
            .await?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| {
                SpendingError::NotFound {
                    entity: "Attachment",
                    id: id.into(),
                }
                .into()
            })
    }

    pub async fn delete_attachment(&self, activity_id: &str, id: &str) -> Result<()> {
        self.attachment(activity_id, id).await?;
        self.repo.delete_attachment(activity_id, id).await
    }
}
