//! The reason `AppState` opens per call: a live-but-idle app or node must not
//! block `restore`, which needs the database file exclusively to leave WAL mode.
//!
//! Its own integration binary because it sets `LINXIV_DATA_DIR`, which is
//! process-global and would race the crate's unit tests. One test for the same
//! reason (`route::storage`'s `backup_and_restore_scenarios` does likewise).

use linxiv_core::service::db_admin;
use linxiv_server::state::AppState;

#[test]
fn idle_state_releases_the_db_but_a_held_connection_still_blocks_restore() {
    let dir = tempfile::TempDir::new().unwrap();
    std::env::set_var("LINXIV_DATA_DIR", dir.path());

    let state = AppState::new().expect("init app state");
    state
        .with_conn(|c| {
            c.execute_batch("CREATE TABLE marker (v TEXT); INSERT INTO marker VALUES ('before');")
        })
        .unwrap();

    let snap = dir.path().join("snap.db");
    state.with_conn(|c| db_admin::backup(c, &snap)).unwrap();
    state
        .with_conn(|c| c.execute("UPDATE marker SET v = 'after'", []))
        .unwrap();

    // The whole point: `state` is alive the entire time. Before per-call opens
    // this was `Conflict: database is in use by another linXiv process`.
    db_admin::restore_closed(&snap).expect("restore over a live-but-idle AppState");

    // And the still-live state reads the restored file, not a deleted inode.
    let v: String = state
        .with_conn(|c| c.query_row("SELECT v FROM marker", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(v, "before");

    // Guards the assertion above from silently passing: a held connection — what
    // `AppState` used to keep for its whole lifetime — is still refused.
    let held = db_admin::open_app_db().unwrap();
    let err = db_admin::restore_closed(&snap).expect_err("a live handle must refuse the restore");
    assert!(
        err.to_string().contains("in use by another linXiv process"),
        "{err}"
    );
    drop(held);
}
