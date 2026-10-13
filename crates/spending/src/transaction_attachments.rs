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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentStorage {
    pub backend: String,
    pub key: String,
    pub preview_key: String,
}

#[derive(Debug, Clone)]
pub struct StoredAttachment {
    pub attachment: TransactionAttachment,
    pub storage: AttachmentStorage,
}

pub const DRAFT_TTL: chrono::Duration = chrono::Duration::hours(24);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentDraft {
    pub id: String,
    pub token: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredDraft {
    pub draft: AttachmentDraft,
    pub storage: AttachmentStorage,
}

pub fn validate_draft_token(token: &str) -> Result<()> {
    uuid::Uuid::try_parse(token)
        .map(|_| ())
        .map_err(|_| SpendingError::InvalidInput {
            message: "Invalid attachment draft token".into(),
        })
        .map_err(Into::into)
}

pub fn storage_for_deleted<'a>(
    pending: Vec<(String, AttachmentStorage)>,
    deleted_ids: impl IntoIterator<Item = &'a str>,
) -> Vec<AttachmentStorage> {
    let deleted: std::collections::HashSet<&str> = deleted_ids.into_iter().collect();
    pending
        .into_iter()
        .filter(|(activity_id, _)| deleted.contains(activity_id.as_str()))
        .map(|(_, storage)| storage)
        .collect()
}

#[async_trait]
pub trait TransactionAttachmentsRepository: Send + Sync {
    async fn storage_for_account(&self, account_id: &str) -> Result<Vec<AttachmentStorage>>;
    async fn storage_for_activities(
        &self,
        activity_ids: &[String],
    ) -> Result<Vec<(String, AttachmentStorage)>>;
    async fn list(&self, activity_id: &str) -> Result<Vec<TransactionAttachment>>;
    async fn find_attachment(
        &self,
        activity_id: &str,
        id: &str,
    ) -> Result<Option<StoredAttachment>>;
    async fn insert_attachment(&self, attachment: StoredAttachment) -> Result<()>;
    async fn delete_attachment(&self, activity_id: &str, id: &str) -> Result<()>;

    async fn list_drafts(&self, token: &str) -> Result<Vec<AttachmentDraft>>;
    async fn find_draft(&self, token: &str, id: &str) -> Result<Option<StoredDraft>>;
    async fn insert_draft(&self, draft: StoredDraft) -> Result<()>;
    async fn delete_draft(&self, token: &str, id: &str) -> Result<()>;
    async fn claim_drafts(&self, token: &str, activity_id: &str) -> Result<usize>;
    async fn discard_drafts(&self, token: &str) -> Result<Vec<AttachmentStorage>>;
    async fn sweep_drafts(&self, cutoff: &str) -> Result<Vec<AttachmentStorage>>;
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

    pub async fn storage_for_account(&self, account_id: &str) -> Result<Vec<AttachmentStorage>> {
        self.repo.storage_for_account(account_id).await
    }

    pub async fn storage_for_activities(
        &self,
        activity_ids: &[String],
    ) -> Result<Vec<(String, AttachmentStorage)>> {
        if activity_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.repo.storage_for_activities(activity_ids).await
    }

    pub async fn list(&self, activity_id: &str) -> Result<Vec<TransactionAttachment>> {
        self.check_access(activity_id).await?;
        self.repo.list(activity_id).await
    }

    pub async fn add_attachment(&self, attachment: StoredAttachment) -> Result<()> {
        self.check_access(&attachment.attachment.activity_id)
            .await?;
        if self
            .repo
            .list(&attachment.attachment.activity_id)
            .await?
            .len()
            >= MAX_ATTACHMENTS
        {
            return Err(SpendingError::InvalidInput {
                message: "Maximum 10 attachments per transaction".into(),
            }
            .into());
        }
        self.repo.insert_attachment(attachment).await
    }

    pub async fn stored(&self, activity_id: &str, id: &str) -> Result<StoredAttachment> {
        self.check_access(activity_id).await?;
        self.repo
            .find_attachment(activity_id, id)
            .await?
            .ok_or_else(|| {
                SpendingError::NotFound {
                    entity: "Attachment",
                    id: id.into(),
                }
                .into()
            })
    }

    pub async fn attachment(&self, activity_id: &str, id: &str) -> Result<TransactionAttachment> {
        Ok(self.stored(activity_id, id).await?.attachment)
    }

    pub async fn delete_attachment(&self, activity_id: &str, id: &str) -> Result<()> {
        self.attachment(activity_id, id).await?;
        self.repo.delete_attachment(activity_id, id).await
    }

    pub async fn list_drafts(&self, token: &str) -> Result<Vec<AttachmentDraft>> {
        validate_draft_token(token)?;
        self.repo.list_drafts(token).await
    }

    pub async fn add_draft(&self, draft: StoredDraft) -> Result<()> {
        validate_draft_token(&draft.draft.token)?;
        if self.repo.list_drafts(&draft.draft.token).await?.len() >= MAX_ATTACHMENTS {
            return Err(SpendingError::InvalidInput {
                message: "Maximum 10 attachments per transaction".into(),
            }
            .into());
        }
        self.repo.insert_draft(draft).await
    }

    pub async fn stored_draft(&self, token: &str, id: &str) -> Result<StoredDraft> {
        validate_draft_token(token)?;
        self.repo.find_draft(token, id).await?.ok_or_else(|| {
            SpendingError::NotFound {
                entity: "Attachment",
                id: id.into(),
            }
            .into()
        })
    }

    pub async fn delete_draft(&self, token: &str, id: &str) -> Result<()> {
        self.stored_draft(token, id).await?;
        self.repo.delete_draft(token, id).await
    }

    pub async fn claim_drafts(&self, token: &str, activity_id: &str) -> Result<usize> {
        validate_draft_token(token)?;
        self.check_access(activity_id).await?;
        self.repo.claim_drafts(token, activity_id).await
    }

    pub async fn discard_drafts(&self, token: &str) -> Result<Vec<AttachmentStorage>> {
        validate_draft_token(token)?;
        self.repo.discard_drafts(token).await
    }

    pub async fn sweep_drafts(&self) -> Result<Vec<AttachmentStorage>> {
        let cutoff = (chrono::Utc::now() - DRAFT_TTL).to_rfc3339();
        self.repo.sweep_drafts(&cutoff).await
    }
}
