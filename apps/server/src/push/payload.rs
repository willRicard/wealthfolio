//! The notification envelope the service worker receives.

use serde::Serialize;

pub(crate) const KIND_TRANSACTION: &str = "transaction.created";
pub(crate) const KIND_BUDGET: &str = "budget.threshold";
pub(crate) const KIND_TEST: &str = "test";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PushPayload {
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    pub tag: String,
    pub url: String,
}

impl PushPayload {
    pub(crate) fn test() -> Self {
        Self {
            kind: KIND_TEST,
            title: "Wealthfolio".to_string(),
            body: "Notifications are working on this device.".to_string(),
            tag: "wf-test".to_string(),
            url: "/settings/notifications".to_string(),
        }
    }
}
