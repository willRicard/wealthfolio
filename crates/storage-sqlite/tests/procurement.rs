#![allow(clippy::unwrap_used, clippy::panic, reason = "test code")]
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::sync::Arc;
use wealthfolio_procurement::{
    items::{ItemsRepositoryTrait, SaveItem},
    purchases::{PurchasesRepositoryTrait, SavePurchase},
};
use wealthfolio_storage_sqlite::{
    db,
    procurement::{items::ItemsRepository, purchases::PurchasesRepository},
    schema::{activities, procurement_item_aliases, procurement_purchases, taxonomy_categories},
    DbPool,
};

struct Fixture {
    items: Arc<ItemsRepository>,
    purchases: PurchasesRepository,
    pool: Arc<DbPool>,
    writer: db::WriteHandle,
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
    diesel::sql_query("INSERT INTO accounts(id,name,currency) VALUES('account','Account','JPY')")
        .execute(&mut conn)
        .unwrap();
    diesel::sql_query("INSERT INTO activities(id,account_id,activity_type,activity_date,currency,notes,created_at,updated_at) VALUES('activity','account','WITHDRAWAL','2026-10-01 00:00:00','JPY','Shop','2026-10-01T00:00:00Z','2026-10-01T00:00:00Z')").execute(&mut conn).unwrap();
    Fixture {
        items: Arc::new(ItemsRepository::new(pool.clone(), writer.clone())),
        purchases: PurchasesRepository::new(pool.clone(), writer.clone()),
        writer,
        pool,
        _dir: dir,
    }
}
fn item() -> SaveItem {
    SaveItem {
        name: "Item".into(),
        brand: None,
        memo: Some("User notes".into()),
        category_id: "cat_groceries".into(),
        subcategory_id: None,
        rating: Some(3.0),
        unit: "個".into(),
        tax_type: "INCLUDED".into(),
        aliases: vec!["Alias".into()],
        draft_id: None,
    }
}
fn purchase(item_id: &str) -> SavePurchase {
    SavePurchase {
        item_id: item_id.into(),
        activity_id: "activity".into(),
        quantity: Decimal::ONE,
        subtotal: Decimal::from(100),
    }
}
#[tokio::test]
async fn procurement_cascades_and_activity_edits_are_live() {
    let f = fixture().await;
    let i = f.items.create(item()).await.unwrap();
    f.purchases
        .set_for_activity("activity", vec![purchase(&i.id)])
        .await
        .unwrap();

    let mut conn = db::get_connection(&f.pool).unwrap();
    diesel::update(activities::table.find("activity"))
        .set((
            activities::activity_date.eq("2026-10-12T00:00:00Z"),
            activities::notes.eq(Some("Edited shop")),
        ))
        .execute(&mut conn)
        .unwrap();
    let rows = f.purchases.list(Some(&i.id)).unwrap();
    assert_eq!(rows[0].date.date_naive().to_string(), "2026-10-12");
    assert_eq!(rows[0].shop.as_deref(), Some("Edited shop"));
    diesel::delete(activities::table.find("activity"))
        .execute(&mut conn)
        .unwrap();
    assert!(f.purchases.list(None).unwrap().is_empty());
    assert!(f.items.get(&i.id).unwrap().is_some());
}
#[tokio::test]
async fn procurement_delete_item_preserves_activity_and_removes_aliases() {
    let f = fixture().await;
    let i = f.items.create(item()).await.unwrap();
    f.purchases
        .set_for_activity("activity", vec![purchase(&i.id)])
        .await
        .unwrap();
    f.items.delete(&i.id).await.unwrap();
    let mut conn = db::get_connection(&f.pool).unwrap();
    assert_eq!(
        activities::table
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        1
    );
    assert_eq!(
        procurement_purchases::table
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        0
    );
    assert_eq!(
        procurement_item_aliases::table
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        0
    );
}
#[tokio::test]
async fn procurement_category_hierarchy_and_decimal_validation() {
    let f = fixture().await;
    let mut input = item();
    input.subcategory_id = Some("cat_shopping_clothing".into());
    assert!(f.items.create(input.clone()).await.is_err());
    input.category_id = "cat_shopping".into();
    let i = f.items.create(input).await.unwrap();
    let mut p = purchase(&i.id);
    p.quantity = Decimal::ZERO;
    assert!(f
        .purchases
        .set_for_activity("activity", vec![p])
        .await
        .is_err());
    let mut p = purchase(&i.id);
    p.subtotal = -Decimal::ONE;
    assert!(f
        .purchases
        .set_for_activity("activity", vec![p])
        .await
        .is_err());
    let mut conn = db::get_connection(&f.pool).unwrap();
    assert_eq!(
        taxonomy_categories::table
            .filter(taxonomy_categories::taxonomy_id.eq("procurement_categories"))
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        38
    );
}

fn photo_service(f: &Fixture) -> Arc<wealthfolio_procurement::photos::PhotoService> {
    Arc::new(
        wealthfolio_procurement::photos::PhotoService::new(
            f._dir.path().join("photos"),
            Arc::new(
                wealthfolio_storage_sqlite::procurement::photo_drafts::PhotoDraftsRepository::new(
                    f.pool.clone(),
                    f.writer.clone(),
                ),
            ),
        )
        .unwrap(),
    )
}
fn items_service(
    f: &Fixture,
    photos: Arc<wealthfolio_procurement::photos::PhotoService>,
) -> wealthfolio_procurement::items::ItemsService {
    let fx = Arc::new(wealthfolio_core::fx::FxService::new(Arc::new(
        wealthfolio_storage_sqlite::fx::FxRepository::new(f.pool.clone(), f.writer.clone()),
    )));
    let settings = Arc::new(wealthfolio_core::settings::SettingsService::new(
        Arc::new(
            wealthfolio_storage_sqlite::settings::SettingsRepository::new(
                f.pool.clone(),
                f.writer.clone(),
            ),
        ),
        fx.clone(),
    ));
    wealthfolio_procurement::items::ItemsService::new(
        f.items.clone(),
        Arc::new(PurchasesRepository::new(f.pool.clone(), f.writer.clone())),
        fx,
        settings,
    )
    .with_photos(photos)
}
async fn upload(
    photos: &wealthfolio_procurement::photos::PhotoService,
) -> wealthfolio_procurement::photo_drafts::PhotoDraft {
    let png = vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
        31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let slot = photos.upload_slot().await.unwrap();
    photos
        .upload("写真.png", "image/png", png, slot)
        .await
        .unwrap()
}
fn draft_count(f: &Fixture) -> i64 {
    use wealthfolio_storage_sqlite::schema::procurement_photo_drafts::dsl::*;
    procurement_photo_drafts
        .count()
        .get_result(&mut db::get_connection(&f.pool).unwrap())
        .unwrap()
}
fn file_count(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                file_count(&path)
            } else {
                1
            }
        })
        .sum()
}
#[tokio::test]
async fn procurement_photo_claim_replace_clone_and_delete() {
    let f = fixture().await;
    let photos = photo_service(&f);
    let service = items_service(&f, photos.clone());
    let first = upload(&photos).await;
    let mut input = item();
    input.draft_id = Some(first.id);
    input.tax_type = "EXCLUDED_10".into();
    let saved = service.create(input.clone()).await.unwrap();
    assert_eq!(draft_count(&f), 0);
    assert!(photos
        .read(saved.photo.as_ref().unwrap(), false)
        .await
        .is_ok());
    let cloned = service.clone_item(&saved.id).await.unwrap();
    assert!(cloned.photo.is_none());
    assert_eq!(cloned.aliases, saved.aliases);
    assert_eq!(cloned.tax_type, "EXCLUDED_10");
    let second = upload(&photos).await;
    input.draft_id = Some(second.id);
    let updated = service.update(&saved.id, input).await.unwrap();
    assert_eq!(draft_count(&f), 0);
    assert!(photos
        .read(saved.photo.as_ref().unwrap(), false)
        .await
        .is_err());
    assert!(photos
        .read(updated.photo.as_ref().unwrap(), true)
        .await
        .is_ok());
    assert_eq!(file_count(&f._dir.path().join("photos")), 2);
    service.delete(&saved.id).await.unwrap();
    assert_eq!(file_count(&f._dir.path().join("photos")), 0);
}
#[tokio::test]
async fn procurement_photo_discard_reselect_and_startup_sweep() {
    use wealthfolio_storage_sqlite::schema::procurement_photo_drafts as drafts;
    let f = fixture().await;
    let photos = photo_service(&f);
    let old = upload(&photos).await;
    photos.discard(&old.id).await.unwrap();
    assert_eq!(draft_count(&f), 0);
    assert_eq!(file_count(&f._dir.path().join("photos")), 0);
    let fresh = upload(&photos).await;
    assert_eq!(draft_count(&f), 1);
    let stale = upload(&photos).await;
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
    diesel::update(drafts::table.find(&stale.id))
        .set(drafts::created_at.eq(cutoff))
        .execute(&mut db::get_connection(&f.pool).unwrap())
        .unwrap();
    photos.sweep_drafts().await.unwrap();
    assert_eq!(draft_count(&f), 1);
    assert!(photos.read(&fresh.photo, true).await.is_ok());
    assert!(photos.read(&stale.photo, false).await.is_err());
}
#[tokio::test]
async fn procurement_failed_photo_commit_removes_both_blobs_and_item_save_rolls_back() {
    let f = fixture().await;
    let photos = photo_service(&f);
    let mut conn = db::get_connection(&f.pool).unwrap();
    diesel::sql_query("CREATE TRIGGER reject_photo BEFORE INSERT ON procurement_photo_drafts BEGIN SELECT RAISE(ABORT, 'test failure'); END").execute(&mut conn).unwrap();
    let slot = photos.upload_slot().await.unwrap();
    let png = vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240,
        31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    assert!(photos
        .upload("photo.png", "image/png", png, slot)
        .await
        .is_err());
    assert_eq!(draft_count(&f), 0);
    assert_eq!(file_count(&f._dir.path().join("photos")), 0);
    diesel::sql_query("DROP TRIGGER reject_photo")
        .execute(&mut conn)
        .unwrap();
    let draft = upload(&photos).await;
    diesel::sql_query("CREATE TRIGGER reject_item BEFORE INSERT ON procurement_items BEGIN SELECT RAISE(ABORT, 'test failure'); END").execute(&mut conn).unwrap();
    let mut input = item();
    input.draft_id = Some(draft.id);
    assert!(f.items.create(input).await.is_err());
    assert_eq!(draft_count(&f), 1);
    assert!(photos.read(&draft.photo, false).await.is_ok());
}

#[tokio::test]
async fn procurement_writes_do_not_dirty_valuation_or_register_sync() {
    use wealthfolio_storage_sqlite::schema::{projection_state, sync_table_state};
    let f = fixture().await;
    let mut conn = db::get_connection(&f.pool).unwrap();
    let before = projection_state::table
        .select((
            projection_state::scope,
            projection_state::dirty_from,
            projection_state::version,
        ))
        .order(projection_state::scope)
        .load::<(String, Option<String>, i64)>(&mut conn)
        .unwrap();
    let saved = f.items.create(item()).await.unwrap();
    f.purchases
        .set_for_activity("activity", vec![purchase(&saved.id)])
        .await
        .unwrap();
    f.items.delete(&saved.id).await.unwrap();
    let after = projection_state::table
        .select((
            projection_state::scope,
            projection_state::dirty_from,
            projection_state::version,
        ))
        .order(projection_state::scope)
        .load::<(String, Option<String>, i64)>(&mut conn)
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        sync_table_state::table
            .filter(sync_table_state::table_name.like("procurement_%"))
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn procurement_migration_rolls_back_with_foreign_keys_disabled() {
    use diesel::connection::SimpleConnection;
    let f = fixture().await;
    let mut conn = db::get_connection(&f.pool).unwrap();
    conn.batch_execute("PRAGMA foreign_keys = OFF;").unwrap();
    conn.batch_execute(include_str!(
        "../migrations/2026-10-17-000001_procurement/down.sql"
    ))
    .unwrap();
    assert_eq!(
        taxonomy_categories::table
            .filter(taxonomy_categories::taxonomy_id.eq("procurement_categories"))
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        0
    );
    assert_eq!(
        taxonomy_categories::table
            .filter(taxonomy_categories::taxonomy_id.eq("spending_categories"))
            .filter(taxonomy_categories::id.eq("cat_groceries"))
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        1
    );
    conn.batch_execute(include_str!(
        "../migrations/2026-10-17-000001_procurement/up.sql"
    ))
    .unwrap();
    assert_eq!(
        taxonomy_categories::table
            .filter(taxonomy_categories::taxonomy_id.eq("procurement_categories"))
            .count()
            .get_result::<i64>(&mut conn)
            .unwrap(),
        38
    );
}

#[tokio::test]
async fn procurement_derives_unit_price_from_stored_subtotal() {
    let f = fixture().await;
    let saved = f.items.create(item()).await.unwrap();
    let mut half_a_kilo = purchase(&saved.id);
    half_a_kilo.quantity = Decimal::new(5, 1);
    half_a_kilo.subtotal = Decimal::from(200);
    f.purchases
        .set_for_activity("activity", vec![half_a_kilo])
        .await
        .unwrap();

    let prices = f.purchases.list(Some(&saved.id)).unwrap();
    assert_eq!(prices[0].quantity, Decimal::new(5, 1));
    assert_eq!(prices[0].unit_price, Decimal::from(400));
    let rows = f.purchases.list_for_activity("activity").unwrap();
    assert_eq!(rows[0].input.subtotal, Decimal::from(200));

    let mut thirds = purchase(&saved.id);
    thirds.quantity = Decimal::from(3);
    thirds.subtotal = Decimal::from(100);
    f.purchases
        .set_for_activity("activity", vec![thirds])
        .await
        .unwrap();
    let prices = f.purchases.list(Some(&saved.id)).unwrap();
    assert_eq!(prices[0].unit_price.round_dp(4), Decimal::new(333333, 4));
    assert_eq!(
        f.purchases.list_for_activity("activity").unwrap()[0]
            .input
            .subtotal,
        Decimal::from(100)
    );
}

#[tokio::test]
async fn procurement_activity_date_edit_changes_latest_price() {
    use wealthfolio_procurement::purchases::stats::fold_prices;
    let f = fixture().await;
    let saved = f.items.create(item()).await.unwrap();
    f.purchases
        .set_for_activity("activity", vec![purchase(&saved.id)])
        .await
        .unwrap();
    let mut conn = db::get_connection(&f.pool).unwrap();
    diesel::sql_query("INSERT INTO activities(id,account_id,activity_type,activity_date,currency,created_at,updated_at) VALUES('later','account','WITHDRAWAL','2026-10-10T00:00:00Z','JPY','2026-10-10T00:00:00Z','2026-10-10T00:00:00Z')").execute(&mut conn).unwrap();
    let mut next = purchase(&saved.id);
    next.activity_id = "later".into();
    next.subtotal = Decimal::from(200);
    f.purchases
        .set_for_activity("later", vec![next])
        .await
        .unwrap();
    let (_, stats) = fold_prices(
        f.purchases.list(Some(&saved.id)).unwrap(),
        "JPY",
        |_, _, _| None,
    )
    .unwrap();
    assert_eq!(stats.latest_price, Some(Decimal::from(200)));
    diesel::update(activities::table.find("activity"))
        .set(activities::activity_date.eq("2026-10-20T00:00:00Z"))
        .execute(&mut conn)
        .unwrap();
    let (_, stats) = fold_prices(
        f.purchases.list(Some(&saved.id)).unwrap(),
        "JPY",
        |_, _, _| None,
    )
    .unwrap();
    assert_eq!(stats.latest_price, Some(Decimal::from(100)));
    assert_eq!(stats.avg_price, Some(Decimal::from(150)));
}

#[tokio::test]
async fn procurement_memo_round_trips_and_clears() {
    let f = fixture().await;
    let saved = f.items.create(item()).await.unwrap();
    assert_eq!(
        f.items.get(&saved.id).unwrap().unwrap().memo.as_deref(),
        Some("User notes")
    );
    let mut input = item();
    input.memo = Some("Updated notes".into());
    f.items.update(&saved.id, input.clone()).await.unwrap();
    assert_eq!(
        f.items.list().unwrap()[0].memo.as_deref(),
        Some("Updated notes")
    );
    input.memo = None;
    f.items.update(&saved.id, input).await.unwrap();
    assert_eq!(f.items.get(&saved.id).unwrap().unwrap().memo, None);
}

#[tokio::test]
async fn procurement_replace_purchases_updates_removes_and_clears() {
    let f = fixture().await;
    let first = f.items.create(item()).await.unwrap();
    let second = f.items.create(item()).await.unwrap();
    let initial = f
        .purchases
        .set_for_activity("activity", vec![purchase(&first.id), purchase(&second.id)])
        .await
        .unwrap();
    assert_eq!(f.purchases.list_for_activity("activity").unwrap().len(), 2);
    let mut updated = purchase(&first.id);
    updated.quantity = Decimal::from(3);
    updated.subtotal = Decimal::ZERO;
    let saved = f
        .purchases
        .set_for_activity("activity", vec![updated])
        .await
        .unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].created_at, initial[0].created_at);
    assert_eq!(saved[0].input.quantity, Decimal::from(3));
    assert_eq!(saved[0].input.subtotal, Decimal::ZERO);
    assert!(f.purchases.list(Some(&second.id)).unwrap().is_empty());
    f.purchases
        .set_for_activity("activity", vec![])
        .await
        .unwrap();
    assert!(f
        .purchases
        .list_for_activity("activity")
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn procurement_failed_replacement_rolls_back_deletions_and_updates() {
    let f = fixture().await;
    let first = f.items.create(item()).await.unwrap();
    let second = f.items.create(item()).await.unwrap();
    f.purchases
        .set_for_activity("activity", vec![purchase(&first.id), purchase(&second.id)])
        .await
        .unwrap();
    let mut updated = purchase(&first.id);
    updated.quantity = Decimal::from(7);
    assert!(f
        .purchases
        .set_for_activity("activity", vec![updated, purchase("missing-item")])
        .await
        .is_err());
    let saved = f.purchases.list_for_activity("activity").unwrap();
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().all(|row| row.input.quantity == Decimal::ONE));
}

#[tokio::test]
async fn procurement_service_validates_replacement_before_writing() {
    let f = fixture().await;
    let fx = Arc::new(wealthfolio_core::fx::FxService::new(Arc::new(
        wealthfolio_storage_sqlite::fx::FxRepository::new(f.pool.clone(), f.writer.clone()),
    )));
    let settings = Arc::new(wealthfolio_core::settings::SettingsService::new(
        Arc::new(
            wealthfolio_storage_sqlite::settings::SettingsRepository::new(
                f.pool.clone(),
                f.writer.clone(),
            ),
        ),
        fx.clone(),
    ));
    let service = wealthfolio_procurement::purchases::PurchasesService::new(
        Arc::new(PurchasesRepository::new(f.pool.clone(), f.writer.clone())),
        f.items.clone(),
        Arc::new(
            wealthfolio_storage_sqlite::activities::ActivityRepository::new(
                f.pool.clone(),
                f.writer.clone(),
            ),
        ),
        fx,
        settings,
    );
    let saved = f.items.create(item()).await.unwrap();
    f.purchases
        .set_for_activity("activity", vec![purchase(&saved.id)])
        .await
        .unwrap();
    let mut mismatch = purchase(&saved.id);
    mismatch.activity_id = "other-activity".into();
    let mut zero_quantity = purchase(&saved.id);
    zero_quantity.quantity = Decimal::ZERO;
    let mut negative_price = purchase(&saved.id);
    negative_price.subtotal = -Decimal::ONE;
    for invalid in [
        vec![mismatch],
        vec![purchase(&saved.id), purchase(&saved.id)],
        vec![zero_quantity],
        vec![negative_price],
        vec![purchase("missing")],
    ] {
        assert!(service.set_for_activity("activity", invalid).await.is_err());
        assert_eq!(service.list_for_activity("activity").unwrap().len(), 1);
    }
    service.set_for_activity("activity", vec![]).await.unwrap();
    assert!(service.list_for_activity("activity").unwrap().is_empty());
}

#[tokio::test]
async fn procurement_tax_type_edits_reinterpret_existing_purchases() {
    use wealthfolio_procurement::purchases::stats::fold_prices;
    let f = fixture().await;
    let saved = f.items.create(item()).await.unwrap();
    let mut input = purchase(&saved.id);
    input.quantity = Decimal::from(2);
    input.subtotal = Decimal::from(1000);
    f.purchases
        .set_for_activity("activity", vec![input])
        .await
        .unwrap();
    for (tax_type, expected) in [
        ("INCLUDED", 500),
        ("EXCLUDED_10", 550),
        ("EXCLUDED_8", 540),
        ("EXEMPT", 500),
    ] {
        let mut updated = item();
        updated.tax_type = tax_type.into();
        let (updated, _) = f.items.update(&saved.id, updated).await.unwrap();
        assert_eq!(updated.tax_type, tax_type);
        let prices = f.purchases.list(Some(&saved.id)).unwrap();
        assert_eq!(prices[0].unit_price, Decimal::from(expected));
        assert_eq!(
            f.purchases.list(None).unwrap()[0].unit_price,
            Decimal::from(expected)
        );
        let (_, stats) = fold_prices(prices, "JPY", |_, _, _| None).unwrap();
        assert_eq!(stats.min_price, Some(Decimal::from(expected)));
        assert_eq!(stats.avg_price, Some(Decimal::from(expected)));
        assert_eq!(stats.latest_price, Some(Decimal::from(expected)));
        let raw = f.purchases.list_for_activity("activity").unwrap();
        assert_eq!(raw[0].input.subtotal, Decimal::from(1000));
        assert_eq!(raw[0].input.quantity, Decimal::from(2));
    }
}

#[tokio::test]
async fn procurement_tax_migration_defaults_existing_items_to_included() {
    let f = fixture().await;
    let saved = f.items.create(item()).await.unwrap();
    let mut conn = db::get_connection(&f.pool).unwrap();
    diesel::sql_query(include_str!(
        "../migrations/2026-10-20-000001_procurement_tax_type/down.sql"
    ))
    .execute(&mut conn)
    .unwrap();
    diesel::sql_query(include_str!(
        "../migrations/2026-10-20-000001_procurement_tax_type/up.sql"
    ))
    .execute(&mut conn)
    .unwrap();
    assert_eq!(
        f.items.get(&saved.id).unwrap().unwrap().tax_type,
        "INCLUDED"
    );
}

#[tokio::test]
async fn procurement_tax_prices_preserve_precision_and_reject_overflow() {
    let f = fixture().await;
    let mut input = item();
    input.tax_type = "EXCLUDED_8".into();
    let saved = f.items.create(input).await.unwrap();
    let mut line = purchase(&saved.id);
    line.subtotal = Decimal::new(1, 2);
    f.purchases
        .set_for_activity("activity", vec![line.clone()])
        .await
        .unwrap();
    assert_eq!(
        f.purchases.list(Some(&saved.id)).unwrap()[0].unit_price,
        Decimal::new(108, 4)
    );
    line.subtotal = Decimal::MAX;
    f.purchases
        .set_for_activity("activity", vec![line])
        .await
        .unwrap();
    assert!(f
        .purchases
        .list(Some(&saved.id))
        .unwrap_err()
        .to_string()
        .contains("subtotal overflow"));
    assert_eq!(
        f.purchases.list_for_activity("activity").unwrap()[0]
            .input
            .subtotal,
        Decimal::MAX
    );
}
