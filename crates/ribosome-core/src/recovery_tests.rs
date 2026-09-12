use crate::{
    contracts::*,
    effects::Runtime,
    host::LocalHost,
    store::Store,
    validation::{decode, now_ms},
};
use rusqlite::params;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn persisted_intent_without_receipt_reconciles_or_remains_unknown_without_reexecution() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("artifact.txt"), "desired output").unwrap();
    let store = Store::open(d.path().join("state.db")).unwrap();
    let host = LocalHost::new(d.path(), BTreeMap::new()).unwrap();
    let runtime = Runtime::new(store, Box::new(host), d.path().join("state")).unwrap();
    let grant:Grant=decode("Grant",json!({"id":"grant","scope":{"client":"client","project":"project"},"mode":"apply","paths":["artifact.txt"],"tools":[],"profiles":["caretaker"],"budget":{"max_calls":1,"max_tokens":"1000","max_cost_microusd":"1000","max_actions":10,"max_work_items":1,"max_depth":1,"deadline_ms":(now_ms()+10000).to_string()},"context":"test","visible_splits":["development"],"allow_export":false})).unwrap();
    runtime.store.register_grant(&grant).unwrap();
    runtime.store.begin_run("run",&grant.id,&decode("AgentRunRequest",json!({"run_id":"run","profile":"caretaker","operator":"proofreading@1","prompt":"recover","provider":"openai","model":"test-model"})).unwrap()).unwrap();
    for (operation, content, expected) in [
        ("matching", "desired output", EffectStatus::Succeeded),
        ("uncertain", "different output", EffectStatus::Unknown),
    ] {
        let action:Action=decode("Action",json!({"operation_id":operation,"kind":"edit","path":"artifact.txt","expected_version":"lost-old-version","content":content})).unwrap();
        let receipt = ActionReceipt {
            operation_id: operation.into(),
            run_id: "run".into(),
            status: EffectStatus::Started,
            action,
            before: vec![],
            after: vec![],
            output: String::new(),
            side_effects: vec![],
            started_ms: now_ms().to_string(),
            finished_ms: None,
            elapsed_ms: None,
            reconciled: false,
            evidence_ref: None,
        };
        runtime
            .store
            .db
            .execute(
                "INSERT INTO effects(id,run_id,grant_id,body) VALUES (?1,'run','grant',?2)",
                params![operation, serde_json::to_string(&receipt).unwrap()],
            )
            .unwrap();
        let result = runtime.lookup("run", operation).unwrap();
        assert_eq!(result.status, expected);
        assert!(result.reconciled);
        assert_eq!(
            std::fs::read_to_string(d.path().join("artifact.txt")).unwrap(),
            "desired output"
        );
    }
    runtime.store.db.execute_batch("CREATE TEMP TRIGGER fail_receipt_event BEFORE INSERT ON events BEGIN SELECT RAISE(FAIL, 'injected evidence write failure'); END;").unwrap();
    let branch = decode(
        "Action",
        json!({"kind":"branch","operation_id":"event-write-failure"}),
    )
    .unwrap();
    assert!(runtime.execute("run", branch).is_err());
    let body: String = runtime
        .store
        .db
        .query_row(
            "SELECT body FROM effects WHERE id='event-write-failure'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let intent: ActionReceipt = serde_json::from_str(&body).unwrap();
    assert_eq!(
        intent.status,
        EffectStatus::Started,
        "a settled receipt must not survive without its evidence event"
    );
    assert_eq!(intent.evidence_ref, None);
    runtime
        .store
        .db
        .execute_batch("DROP TRIGGER fail_receipt_event")
        .unwrap();
    assert_eq!(
        runtime.lookup("run", "event-write-failure").unwrap().status,
        EffectStatus::Unknown
    );
}
