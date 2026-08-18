use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::database_connections::{self, DatabaseConnectionRecord};
use rusqlite::Connection;
use uuid::Uuid;

const WORKSPACE: &str = "local";
const OTHER_WORKSPACE: &str = "other";

fn database() -> Connection {
    let mut connection = Connection::open_in_memory().expect("open memory database");
    migrate(&mut connection).expect("migrate database");
    connection
}

fn record(display_name: &str) -> DatabaseConnectionRecord {
    DatabaseConnectionRecord {
        id: Uuid::new_v4(),
        display_name: display_name.to_string(),
        host: "192.168.1.10".to_string(),
        port: 1433,
        database_name: "SteelWorks".to_string(),
        username: "sa".to_string(),
        timeout_ms: 10_000,
        enabled: true,
    }
}

#[test]
fn database_connection_crud_round_trip() {
    let mut conn = database();
    let first = record("3 号高炉");
    let second = record("连铸产线");

    database_connections::save(&mut conn, WORKSPACE, &first).expect("save first");
    database_connections::save(&mut conn, WORKSPACE, &second).expect("save second");

    let listed = database_connections::list(&conn, WORKSPACE).expect("list");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].display_name, "3 号高炉");
    assert_eq!(listed[0].host, first.host);
    assert_eq!(listed[0].port, 1433);

    let fetched = database_connections::get(&conn, WORKSPACE, first.id)
        .expect("get")
        .expect("record exists");
    assert_eq!(fetched.database_name, "SteelWorks");

    database_connections::delete(&mut conn, WORKSPACE, first.id).expect("delete");
    assert!(database_connections::get(&conn, WORKSPACE, first.id)
        .expect("get after delete")
        .is_none());
    assert_eq!(
        database_connections::list(&conn, WORKSPACE)
            .expect("list after delete")
            .len(),
        1
    );
}

#[test]
fn database_connections_are_workspace_scoped() {
    let mut conn = database();
    let owned = record("厂区 A");

    database_connections::save(&mut conn, WORKSPACE, &owned).expect("save");
    assert!(database_connections::list(&conn, OTHER_WORKSPACE)
        .expect("list other workspace")
        .is_empty());
    assert!(database_connections::get(&conn, OTHER_WORKSPACE, owned.id)
        .expect("get other workspace")
        .is_none());
    assert!(
        database_connections::delete(&mut conn, OTHER_WORKSPACE, owned.id).is_err(),
        "other workspace must not delete our connection"
    );
}

#[test]
fn database_connection_update_preserves_owner_workspace() {
    let mut conn = database();
    let original = record("轧机");
    database_connections::save(&mut conn, WORKSPACE, &original).expect("save");

    let mut renamed = original.clone();
    renamed.display_name = "热轧轧机".to_string();
    database_connections::save(&mut conn, WORKSPACE, &renamed).expect("update");

    let updated = database_connections::get(&conn, WORKSPACE, original.id)
        .expect("get")
        .expect("exists");
    assert_eq!(updated.display_name, "热轧轧机");

    assert!(
        database_connections::save(&mut conn, OTHER_WORKSPACE, &renamed).is_err(),
        "saving from another workspace must be rejected"
    );
}
