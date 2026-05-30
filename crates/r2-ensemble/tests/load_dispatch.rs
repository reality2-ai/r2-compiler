//! Happy-path: load an ensemble, dispatch an envelope, observe the
//! resulting outbound event in the capturing sink.

mod common;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use r2_dispatch::{DispatchEnvelope, DispatchTarget};
use r2_ensemble::{CapturingSink, EnsembleRegistry, EnsembleStatus};
use r2_fnv::r2_hash;

use common::{make_score, EchoFactory};

#[tokio::test]
async fn load_dispatch_emits_outbound() {
    let _ = env_logger::builder().is_test(true).try_init();

    let reg = Arc::new(EnsembleRegistry::new());
    let fac = Arc::new(EchoFactory::new());
    reg.register_factory(fac.clone());

    let sink = Arc::new(CapturingSink::default());
    reg.set_sink(sink.clone());

    let score = make_score("notekeeper", "echo", &["note.create"]);
    let id = reg.load(score).expect("load ok");
    assert_eq!(id, "notekeeper");
    assert_eq!(reg.list(), vec!["notekeeper".to_string()]);
    assert_eq!(
        reg.info("notekeeper").map(|e| e.status()),
        Some(EnsembleStatus::Healthy)
    );

    let event_hash = r2_hash("note.create").unwrap();
    let env = DispatchEnvelope {
        originator: 0xCAFE_BABE,
        target_hive: 0,
        target_group: 0,
        event_hash,
        payload: b"hello",
        msg_id: 42,
        mcu_origin: false,
        received_at: 0,
        trust_group: None,
    };

    reg.dispatch(env).await.expect("dispatch ok");

    assert_eq!(fac.seen.load(Ordering::SeqCst), 1);
    let captured = sink.events.lock();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].event_hash, event_hash);
    assert_eq!(captured[0].payload, b"hello");
    assert_eq!(captured[0].source_ensemble, "notekeeper");
}

#[tokio::test]
async fn dispatch_unknown_event_returns_no_handler() {
    let reg = Arc::new(EnsembleRegistry::new());
    reg.register_factory(Arc::new(EchoFactory::new()));
    reg.load(make_score("e1", "echo", &["a.b"])).unwrap();

    let env = DispatchEnvelope {
        originator: 0,
        target_hive: 0,
        target_group: 0,
        event_hash: r2_hash("never.subscribed").unwrap(),
        payload: &[],
        msg_id: 0,
        mcu_origin: false,
        received_at: 0,
        trust_group: None,
    };
    let r = reg.dispatch(env).await;
    assert!(matches!(r, Err(r2_dispatch::DispatchError::NoHandler)));
}

#[tokio::test]
async fn duplicate_load_rejected() {
    let reg = EnsembleRegistry::new();
    reg.register_factory(Arc::new(EchoFactory::new()));
    reg.load(make_score("dup", "echo", &["x.y"])).unwrap();
    let r = reg.load(make_score("dup", "echo", &["x.y"]));
    assert!(matches!(r, Err(r2_ensemble::LoadError::AlreadyLoaded(_))));
}

#[tokio::test]
async fn stop_removes_subscriptions() {
    let reg = Arc::new(EnsembleRegistry::new());
    reg.register_factory(Arc::new(EchoFactory::new()));
    reg.load(make_score("s1", "echo", &["e.a"])).unwrap();
    reg.stop("s1").unwrap();

    let env = DispatchEnvelope {
        originator: 0,
        target_hive: 0,
        target_group: 0,
        event_hash: r2_hash("e.a").unwrap(),
        payload: &[],
        msg_id: 0,
        mcu_origin: false,
        received_at: 0,
        trust_group: None,
    };
    let r = reg.dispatch(env).await;
    assert!(matches!(r, Err(r2_dispatch::DispatchError::NoHandler)));
}
