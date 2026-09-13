use ribosome_core::{store::Store, validation::decode};
use rusqlite::Connection;
use serde_json::json;

#[test]
fn schema_twenty_upgrade_preserves_exposure_and_rolls_back_partial_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state.db");
    let store = Store::open(&path).unwrap();
    let grant = decode("Grant", json!({"id":"owner","scope":{"client":"test","project":"qualification"},"mode":"observe","paths":[],"tools":[],"profiles":["experimenter"],"budget":{"max_calls":4,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":0,"max_work_items":0,"max_depth":0,"deadline_ms":"9999999999999"},"context":"test","visible_splits":["development"],"allow_export":false})).unwrap();
    store.register_grant(&grant).unwrap();
    drop(store);
    let db = Connection::open(&path).unwrap();
    // Recreate the schema immediately before the R6 migration.
    db.execute_batch("DROP TABLE evaluation_sources; DROP TABLE protected_exposures; ALTER TABLE archive DROP COLUMN evidence; PRAGMA user_version=20;").unwrap();
    db.execute("INSERT INTO experiments(id,grant_id,policy_id,policy) VALUES('prior-study','owner','prior-policy',?1)", [json!({"cases":[{"id":"private-case","family":"consumed-family","split":"holdout"}]}).to_string()]).unwrap();
    db.execute_batch("CREATE TABLE evaluation_sources(existing TEXT)")
        .unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        20
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('archive') WHERE name='evidence'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='protected_exposures'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TABLE evaluation_sources").unwrap();
    let upgraded = Store::open(&path).unwrap();
    assert_eq!(upgraded.grant("owner").unwrap(), grant);
    drop(upgraded);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        21
    );
    let exposure: (String, String) = db.query_row("SELECT family,experiment_id FROM protected_exposures WHERE client='test' AND project='qualification'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(exposure, ("consumed-family".into(), "prior-study".into()));
    assert!(Store::open(&path).is_ok());
}
