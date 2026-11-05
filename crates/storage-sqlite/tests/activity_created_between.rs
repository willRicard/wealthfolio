#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]

use std::sync::Arc;

use chrono::{DateTime, Utc};
use diesel::prelude::*;
use wealthfolio_core::activities::ActivityRepositoryTrait;
use wealthfolio_storage_sqlite::{activities::ActivityRepository, db};

fn at(timestamp: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(timestamp)
        .unwrap()
        .with_timezone(&Utc)
}

struct Fixture {
    repository: ActivityRepository,
    _dir: tempfile::TempDir,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let access = db::DbAccess::plaintext(dir.path().join("app.db").to_str().unwrap());
    access.prepare().unwrap();
    access.run_migrations().unwrap();
    let pool = access.create_pool().unwrap();
    let (writer, _task) = db::write_actor::spawn_writer_with_sync_state(
        (*pool).clone(),
        Arc::new(|| {}),
        Arc::default(),
    )
    .unwrap();

    let mut conn = db::get_connection(&pool).unwrap();
    diesel::sql_query(
        "INSERT INTO accounts(id,name,currency) VALUES('acct','Account','CAD'),\
         ('archived','Archived','CAD')",
    )
    .execute(&mut conn)
    .unwrap();
    diesel::sql_query("UPDATE accounts SET is_archived = 1 WHERE id = 'archived'")
        .execute(&mut conn)
        .unwrap();

    // (id, account, activity_date, created_at)
    let rows = [
        (
            "old",
            "acct",
            "2026-10-01T00:00:00+00:00",
            "2026-10-01T09:00:00+00:00",
        ),
        (
            "backdated",
            "acct",
            "2026-01-05T00:00:00+00:00",
            "2026-11-04T12:00:05+00:00",
        ),
        (
            "inside",
            "acct",
            "2026-11-04T00:00:00+00:00",
            "2026-11-04T12:00:10+00:00",
        ),
        (
            "boundary",
            "acct",
            "2026-11-04T00:00:00+00:00",
            "2026-11-04T12:00:30+00:00",
        ),
        (
            "future",
            "acct",
            "2026-11-04T00:00:00+00:00",
            "2026-11-04T12:01:00+00:00",
        ),
        (
            "archived-row",
            "archived",
            "2026-11-04T00:00:00+00:00",
            "2026-11-04T12:00:15+00:00",
        ),
    ];
    for (id, account, activity_date, created_at) in rows {
        diesel::sql_query(format!(
            "INSERT INTO activities(id,account_id,activity_type,activity_date,currency,amount,\
             notes,created_at,updated_at) VALUES('{id}','{account}','WITHDRAWAL',\
             '{activity_date}','CAD','10','Merchant','{created_at}','{created_at}')"
        ))
        .execute(&mut conn)
        .unwrap();
    }

    Fixture {
        repository: ActivityRepository::new(pool.clone(), writer),
        _dir: dir,
    }
}

#[tokio::test]
async fn the_window_is_exclusive_at_the_start_and_inclusive_at_the_end() {
    let fixture = fixture().await;

    let found = fixture
        .repository
        .get_activities_created_between(
            at("2026-11-04T12:00:05+00:00"),
            at("2026-11-04T12:00:30+00:00"),
            50,
        )
        .unwrap();

    let ids: Vec<&str> = found.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, vec!["boundary", "inside"], "newest first");
}

#[tokio::test]
async fn a_back_dated_entry_is_found_by_its_write_time() {
    let fixture = fixture().await;

    let found = fixture
        .repository
        .get_activities_created_between(
            at("2026-11-04T00:00:00+00:00"),
            at("2026-11-04T12:00:06+00:00"),
            50,
        )
        .unwrap();

    let ids: Vec<&str> = found.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, vec!["backdated"]);
    assert_eq!(found[0].activity_date, at("2026-01-05T00:00:00+00:00"));
}

#[tokio::test]
async fn archived_accounts_are_excluded() {
    let fixture = fixture().await;

    let found = fixture
        .repository
        .get_activities_created_between(
            at("2026-11-04T12:00:06+00:00"),
            at("2026-11-04T12:00:20+00:00"),
            50,
        )
        .unwrap();

    let ids: Vec<&str> = found.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, vec!["inside"]);
    assert!(!ids.contains(&"archived-row"));
}

#[tokio::test]
async fn the_limit_caps_the_read_and_keeps_the_newest() {
    let fixture = fixture().await;

    let found = fixture
        .repository
        .get_activities_created_between(
            at("2026-01-01T00:00:00+00:00"),
            at("2026-12-01T00:00:00+00:00"),
            2,
        )
        .unwrap();

    let ids: Vec<&str> = found.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, vec!["future", "boundary"]);
}

#[tokio::test]
async fn an_empty_window_returns_nothing() {
    let fixture = fixture().await;

    let found = fixture
        .repository
        .get_activities_created_between(
            at("2026-11-04T13:00:00+00:00"),
            at("2026-11-04T14:00:00+00:00"),
            50,
        )
        .unwrap();

    assert!(found.is_empty());
}
