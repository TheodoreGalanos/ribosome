use ribosome_core::{
    contracts::*,
    store::Store,
    validation::{id, validate},
};
use serde_json::json;
use std::time::Instant;

fn event(producer: &str, sequence: u32) -> Event {
    Event {
        id: id(),
        scope: Scope {
            client: "measure".into(),
            project: "local".into(),
        },
        run_id: "bench".into(),
        producer: producer.into(),
        sequence: sequence.to_string(),
        kind: "measurement".into(),
        timestamp_ms: "1".into(),
        parents: vec![],
        correlation: "benchmark".into(),
        artifacts: vec![],
        payload: serde_json::Map::new(),
        provenance: Provenance {
            origin: Origin::Observed,
            source_refs: vec![],
            scenario_family: "storage-timing".into(),
            split: Split::Development,
            limitations: vec![],
        },
    }
}
fn main() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("measure.db");
    let store = Store::open(&path).unwrap();
    validate("Event", &serde_json::to_value(event("warmup", 0)).unwrap()).unwrap();
    let start = Instant::now();
    for sequence in 0..500 {
        store.ingest(&event("single", sequence)).unwrap();
    }
    let single = start.elapsed().as_secs_f64() * 1000.0;
    let first = Store::open(&path).unwrap();
    let second = Store::open(&path).unwrap();
    let start = Instant::now();
    let workers = [("first", first), ("second", second)]
        .into_iter()
        .map(|(producer, store)| {
            std::thread::spawn(move || {
                for sequence in 0..500 {
                    store.ingest(&event(producer, sequence)).unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    let contended = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "{}",
        json!({"kind":"infrastructure-only","model_time_ms":0,"single_writer":{"events":500,"elapsed_ms":single},"two_writers":{"events":1000,"elapsed_ms":contended},"note":"Warm schema validators; local SQLite WAL; one sample, not a throughput guarantee."})
    );
}
