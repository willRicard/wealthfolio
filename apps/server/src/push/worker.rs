use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::Utc;
use tokio::sync::broadcast::error::RecvError;

use super::payload::{PushPayload, KIND_BUDGET, KIND_TRANSACTION};
use super::store::budget_level_key;
use crate::events::{
    ServerEvent, BROKER_SYNC_COMPLETE, MARKET_SYNC_COMPLETE, PORTFOLIO_UPDATE_COMPLETE,
};
use crate::main_lib::AppState;

pub(crate) const BUDGET_THRESHOLDS: [u8; 3] = [100, 80, 50];

const DEBOUNCE: Duration = Duration::from_secs(5);

const IDLE_TICK: Duration = Duration::from_secs(15 * 60);

const CATEGORISATION_GRACE_SECS: i64 = 10;

const MAX_TRANSACTION_NOTIFICATIONS: usize = 3;

const TRANSACTION_SCAN_LIMIT: usize = 200;

pub(crate) fn budget_level(actual: f64, target: f64) -> u8 {
    if !actual.is_finite() || !target.is_finite() || target <= 0.0 || actual <= 0.0 {
        return 0;
    }
    let percent = actual / target * 100.0;
    BUDGET_THRESHOLDS
        .iter()
        .copied()
        .find(|threshold| percent >= f64::from(*threshold))
        .unwrap_or(0)
}

pub(crate) fn crossing(previous: u8, current: u8) -> Option<u8> {
    (current > previous).then_some(current)
}

fn wakes_evaluator(event: &ServerEvent) -> bool {
    matches!(
        event.name,
        PORTFOLIO_UPDATE_COMPLETE | MARKET_SYNC_COMPLETE | BROKER_SYNC_COMPLETE
    )
}

pub(crate) fn start_push_worker(state: Arc<AppState>) -> Option<tokio::task::JoinHandle<()>> {
    state.push_service.as_ref()?;

    Some(tokio::spawn(async move {
        let mut events = state.event_bus.subscribe();
        let mut ticker = tokio::time::interval(IDLE_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        tracing::info!("Push notification evaluator started");

        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                received = events.recv() => match received {
                    Ok(event) => {
                        if !wakes_evaluator(&event) {
                            continue;
                        }
                        tokio::time::sleep(DEBOUNCE).await;
                        while events.try_recv().is_ok() {}
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                },
            }

            if let Err(error) = run_pass(&state).await {
                tracing::warn!("Push notification evaluation failed: {error}");
            }
        }

        tracing::info!("Push notification evaluator stopped");
    }))
}

async fn run_pass(state: &Arc<AppState>) -> anyhow::Result<()> {
    let Some(push) = state.push_service.as_ref() else {
        return Ok(());
    };

    let file = push.store().read()?;
    if file.subscriptions.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    let scan_until = now - chrono::Duration::seconds(CATEGORISATION_GRACE_SECS);

    let mut payloads = Vec::new();

    let previous_watermark = file.state.last_activity_seen_at;
    let mut next_watermark = previous_watermark;

    match previous_watermark {
        None => next_watermark = Some(now),
        Some(since) if scan_until > since => {
            next_watermark = Some(scan_until);
            if file.preferences.transactions {
                match transaction_payloads(state, since, scan_until).await {
                    Ok(mut found) => payloads.append(&mut found),
                    Err(error) => {
                        next_watermark = previous_watermark;
                        tracing::warn!("Could not read new transactions for notification: {error}");
                    }
                }
            }
        }
        Some(_) => {}
    }

    // ---- budget thresholds ------------------------------------------------
    let mut budget_update = None;
    if file.preferences.budgets {
        match budget_crossings(state, &file).await {
            Ok(Some((period_key, levels, mut found))) => {
                payloads.append(&mut found);
                budget_update = Some((period_key, levels));
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!("Could not evaluate budget thresholds for notification: {error}");
            }
        }
    }

    push.store().with_file(|file| {
        file.state.last_activity_seen_at = next_watermark;
        if let Some((period_key, levels)) = budget_update {
            file.state.budget_period_key = Some(period_key);
            file.state.budget_levels = levels;
        }
        Ok(())
    })?;

    if payloads.is_empty() {
        return Ok(());
    }
    push.send_to_all(&payloads).await?;
    Ok(())
}

async fn transaction_payloads(
    state: &Arc<AppState>,
    since: chrono::DateTime<Utc>,
    until: chrono::DateTime<Utc>,
) -> anyhow::Result<Vec<PushPayload>> {
    let created = state
        .activity_repository
        .get_activities_created_between(since, until, TRANSACTION_SCAN_LIMIT)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if created.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = created.into_iter().map(|activity| activity.id).collect();
    let cash = state
        .cash_activity_service
        .get_by_activity_ids(&ids)
        .await?;
    if cash.is_empty() {
        return Ok(Vec::new());
    }

    let category_names = category_names(state)?;

    if cash.len() > MAX_TRANSACTION_NOTIFICATIONS {
        let total: usize = cash.len();
        return Ok(vec![PushPayload {
            kind: KIND_TRANSACTION,
            title: format!("{total} new transactions"),
            body: "Open Wealthfolio to review them.".to_string(),
            tag: "wf-transactions".to_string(),
            url: "/spending/transactions".to_string(),
        }]);
    }

    Ok(cash
        .iter()
        .map(|entry| {
            let merchant = merchant_of(entry);
            let category = entry
                .assignments
                .first()
                .map(|assignment| {
                    category_names
                        .get(&budget_level_key(
                            &assignment.taxonomy_id,
                            &assignment.category_id,
                        ))
                        .cloned()
                        .unwrap_or_else(|| "Uncategorised".to_string())
                })
                .unwrap_or_else(|| "Uncategorised".to_string());
            let amount = format_amount(
                entry
                    .activity
                    .amount
                    .map(|amount| amount.abs())
                    .unwrap_or_default(),
                &entry.activity.currency,
            );

            PushPayload {
                kind: KIND_TRANSACTION,
                title: "New transaction".to_string(),
                body: format!("{merchant} · {category} · {amount}"),
                tag: format!("wf-activity-{}", entry.activity.id),
                url: "/spending/transactions".to_string(),
            }
        })
        .collect())
}

#[allow(clippy::type_complexity)]
async fn budget_crossings(
    state: &Arc<AppState>,
    file: &super::store::PushFile,
) -> anyhow::Result<
    Option<(
        String,
        std::collections::BTreeMap<String, u8>,
        Vec<PushPayload>,
    )>,
> {
    let currency = state.base_currency()?;
    let timezone = state.timezone()?;
    let snapshot = state
        .budget_service
        .get_readonly(None, &currency, &timezone)
        .await?;

    let computed = snapshot.computed;
    let period_rolled_over = file
        .state
        .budget_period_key
        .as_deref()
        .is_some_and(|previous| previous != computed.period_key);
    let previous_levels = if period_rolled_over {
        Default::default()
    } else {
        file.state.budget_levels.clone()
    };

    let rows = computed
        .group_rows
        .iter()
        .flat_map(|group| group.categories.iter())
        .chain(computed.ungrouped_rows.iter());

    let mut levels = std::collections::BTreeMap::new();
    let mut payloads = Vec::new();

    for row in rows {
        let key = budget_level_key(&row.taxonomy_id, &row.category_id);
        let current = budget_level(row.actual, row.target);
        let previous = previous_levels.get(&key).copied().unwrap_or(0);
        if current > 0 {
            levels.insert(key.clone(), current);
        }

        let Some(level) = crossing(previous, current) else {
            continue;
        };
        payloads.push(PushPayload {
            kind: KIND_BUDGET,
            title: format!("{} budget at {level}%", row.name),
            body: format!(
                "{} of {} spent",
                format_amount_f64(row.actual, &computed.currency),
                format_amount_f64(row.target, &computed.currency)
            ),
            tag: format!("wf-budget-{}-{}", computed.period_key, row.category_id),
            url: format!("/spending/budget?month={}", computed.period_key),
        });
    }

    Ok(Some((computed.period_key, levels, payloads)))
}

fn category_names(state: &Arc<AppState>) -> anyhow::Result<HashMap<String, String>> {
    let mut names = HashMap::new();
    for taxonomy in state.taxonomy_service.get_taxonomies_with_categories()? {
        for category in taxonomy.categories {
            names.insert(
                budget_level_key(&taxonomy.taxonomy.id, &category.id),
                category.name,
            );
        }
    }
    Ok(names)
}

fn merchant_of(entry: &wealthfolio_spending::cash_activities::CashActivity) -> String {
    entry
        .activity
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|notes| !notes.is_empty())
        .map(|notes| notes.chars().take(60).collect::<String>())
        .unwrap_or_else(|| "Transaction".to_string())
}

fn format_amount(amount: rust_decimal::Decimal, currency: &str) -> String {
    format!("{} {}", amount.round_dp(2).normalize(), currency)
}

fn format_amount_f64(amount: f64, currency: &str) -> String {
    format!("{amount:.2} {currency}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

    use super::*;

    #[test]
    fn the_ladder_reports_the_highest_threshold_reached() {
        assert_eq!(budget_level(0.0, 100.0), 0);
        assert_eq!(budget_level(49.9, 100.0), 0);
        assert_eq!(budget_level(50.0, 100.0), 50);
        assert_eq!(budget_level(79.9, 100.0), 50);
        assert_eq!(budget_level(80.0, 100.0), 80);
        assert_eq!(budget_level(99.9, 100.0), 80);
        assert_eq!(budget_level(100.0, 100.0), 100);
        assert_eq!(budget_level(250.0, 100.0), 100);
    }

    #[test]
    fn an_unbudgeted_or_nonsensical_category_has_no_ladder() {
        assert_eq!(budget_level(50.0, 0.0), 0);
        assert_eq!(budget_level(50.0, -10.0), 0);
        assert_eq!(budget_level(f64::NAN, 100.0), 0);
        assert_eq!(budget_level(50.0, f64::NAN), 0);
        assert_eq!(budget_level(f64::INFINITY, 100.0), 0);
        assert_eq!(budget_level(-20.0, 100.0), 0);
    }

    #[test]
    fn a_jump_past_several_thresholds_announces_only_the_highest() {
        let previous = budget_level(40.0, 100.0);
        let current = budget_level(85.0, 100.0);
        assert_eq!(crossing(previous, current), Some(80));
    }

    #[test]
    fn staying_at_the_same_level_is_silent() {
        assert_eq!(crossing(80, 80), None);
        // 85% → 90% is still the 80% band.
        assert_eq!(
            crossing(budget_level(85.0, 100.0), budget_level(90.0, 100.0)),
            None
        );
    }

    #[test]
    fn dropping_below_re_arms_the_threshold() {
        let at_ninety = budget_level(90.0, 100.0);
        assert_eq!(crossing(0, at_ninety), Some(80));
        let after_delete = budget_level(40.0, 100.0);
        assert_eq!(crossing(at_ninety, after_delete), None);
        assert_eq!(after_delete, 0);
        assert_eq!(crossing(after_delete, budget_level(90.0, 100.0)), Some(80));
    }

    #[test]
    fn each_threshold_fires_once_as_spending_climbs() {
        let mut previous = 0;
        let mut fired = Vec::new();
        for actual in [10.0, 55.0, 60.0, 81.0, 95.0, 100.0, 140.0] {
            let current = budget_level(actual, 100.0);
            if let Some(level) = crossing(previous, current) {
                fired.push(level);
            }
            previous = current;
        }
        assert_eq!(fired, vec![50, 80, 100]);
    }

    #[test]
    fn only_completion_events_wake_the_evaluator() {
        assert!(wakes_evaluator(&ServerEvent::new(
            PORTFOLIO_UPDATE_COMPLETE
        )));
        assert!(wakes_evaluator(&ServerEvent::new(MARKET_SYNC_COMPLETE)));
        assert!(wakes_evaluator(&ServerEvent::new(BROKER_SYNC_COMPLETE)));
        assert!(!wakes_evaluator(&ServerEvent::new(
            crate::events::PORTFOLIO_UPDATE_START
        )));
        assert!(!wakes_evaluator(&ServerEvent::new(
            crate::events::ASSET_ENRICHMENT_PROGRESS
        )));
    }

    #[test]
    fn thresholds_are_ordered_highest_first_for_the_ladder_lookup() {
        let mut sorted = BUDGET_THRESHOLDS;
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        assert_eq!(sorted, BUDGET_THRESHOLDS);
    }

    #[test]
    fn amounts_are_rendered_with_their_currency() {
        assert_eq!(
            format_amount(rust_decimal::Decimal::new(1234, 2), "CAD"),
            "12.34 CAD"
        );
        assert_eq!(format_amount_f64(80.0, "EUR"), "80.00 EUR");
    }
}
