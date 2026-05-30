//! Supervisor behaviour: panics caught, sentants restarted, intensity
//! cap escalates to `Failed`.

mod common;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use r2_dispatch::{DispatchEnvelope, DispatchError, DispatchTarget};
use r2_ensemble::{
    BackoffPolicy, EnsembleRegistry, EnsembleStatus, RestartStrategy, SupervisionConfig,
};
use r2_fnv::r2_hash;

use common::{make_score, EchoFactory};

fn envelope(event_hash: u32) -> DispatchEnvelope<'static> {
    DispatchEnvelope {
        originator: 0,
        target_hive: 0,
        target_group: 0,
        event_hash,
        payload: &[],
        msg_id: 0,
        mcu_origin: false,
        received_at: 0,
        trust_group: None,
    }
}

#[tokio::test]
async fn panic_caught_and_sentant_restarted() {
    let _ = env_logger::builder().is_test(true).try_init();

    let reg = Arc::new(EnsembleRegistry::new());
    let fac = Arc::new(EchoFactory::new());
    reg.register_factory(fac.clone());

    let cfg = SupervisionConfig {
        strategy: RestartStrategy::OneForOne,
        max_restarts: 5,
        period: Duration::from_secs(60),
        backoff: BackoffPolicy::Constant { delay_ms: 20 },
    };
    let score = make_score("e1", "echo", &["go"]);
    reg.load_with(score, cfg).unwrap();

    let h = r2_hash("go").unwrap();

    // Arm the panic. First dispatch panics; supervisor restarts.
    fac.will_panic.store(true, Ordering::SeqCst);
    let r = reg.dispatch(envelope(h)).await;
    assert!(matches!(r, Err(DispatchError::Rejected)));

    // While restarting, dispatch returns Backpressure.
    let r = reg.dispatch(envelope(h)).await;
    assert!(matches!(r, Err(DispatchError::Backpressure) | Ok(())));

    // Wait for restart to complete.
    reg.await_quiescent(Duration::from_secs(1)).await;
    let info = reg.info("e1").unwrap();
    assert_eq!(info.status(), EnsembleStatus::Healthy);

    // After restart, dispatch succeeds again.
    let r = reg.dispatch(envelope(h)).await;
    assert!(r.is_ok(), "post-restart dispatch should succeed: {r:?}");

    // Factory was called twice: initial load + restart.
    assert_eq!(fac.builds.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn intensity_exceeded_marks_ensemble_failed() {
    let _ = env_logger::builder().is_test(true).try_init();

    let reg = Arc::new(EnsembleRegistry::new());
    let fac = Arc::new(EchoFactory::new());
    reg.register_factory(fac.clone());

    let cfg = SupervisionConfig {
        strategy: RestartStrategy::OneForOne,
        max_restarts: 2,
        period: Duration::from_secs(60),
        backoff: BackoffPolicy::Constant { delay_ms: 5 },
    };
    reg.load_with(make_score("e1", "echo", &["go"]), cfg).unwrap();

    let h = r2_hash("go").unwrap();

    // Crash three times in a row; cap is 2 → on the 3rd record the
    // ensemble escalates to Failed.
    for _ in 0..3 {
        fac.will_panic.store(true, Ordering::SeqCst);
        let _ = reg.dispatch(envelope(h)).await;
        // Wait for restart attempt to finish.
        reg.await_quiescent(Duration::from_millis(200)).await;
    }

    let info = reg.info("e1").unwrap();
    assert_eq!(info.status(), EnsembleStatus::Failed);

    // Dispatch to a Failed ensemble returns NoHandler.
    let r = reg.dispatch(envelope(h)).await;
    assert!(matches!(r, Err(DispatchError::NoHandler)));
}

#[tokio::test]
async fn reset_brings_failed_ensemble_back_to_healthy() {
    let _ = env_logger::builder().is_test(true).try_init();

    let reg = Arc::new(EnsembleRegistry::new());
    let fac = Arc::new(EchoFactory::new());
    reg.register_factory(fac.clone());

    let cfg = SupervisionConfig {
        strategy: RestartStrategy::OneForOne,
        max_restarts: 1,
        period: Duration::from_secs(60),
        backoff: BackoffPolicy::Constant { delay_ms: 5 },
    };
    reg.load_with(make_score("e1", "echo", &["go"]), cfg).unwrap();

    let h = r2_hash("go").unwrap();
    for _ in 0..2 {
        fac.will_panic.store(true, Ordering::SeqCst);
        let _ = reg.dispatch(envelope(h)).await;
        reg.await_quiescent(Duration::from_millis(100)).await;
    }
    assert_eq!(reg.info("e1").unwrap().status(), EnsembleStatus::Failed);

    reg.reset("e1").unwrap();
    assert_eq!(reg.info("e1").unwrap().status(), EnsembleStatus::Healthy);

    // Operator has fixed whatever was making the sentant panic; ensure
    // the next event won't trip it.
    fac.will_panic.store(false, Ordering::SeqCst);

    // After reset, a clean dispatch succeeds.
    let r = reg.dispatch(envelope(h)).await;
    assert!(r.is_ok(), "after reset dispatch should succeed: {r:?}");
}
