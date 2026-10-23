//! Workspace schema migrations (NFR-03, ARC-34).
//!
//! Migrations run on raw JSON before typed parsing, one version at a time. Version 1 is the
//! first schema, so the chain is empty for now; add `(from, fn)` entries as the schema evolves.

use serde_json::Value;

use mdm_core::{Message, SCHEMA_VERSION};

use crate::error::StoreError;

/// Upgrades a document from schema `from` to `from + 1`.
pub type MigrationFn = fn(Value) -> Result<Value, String>;

pub const MIGRATIONS: &[(u32, MigrationFn)] = &[];

/// Read `schemaVersion` from a raw document.
pub fn schema_version(doc: &Value) -> Option<u32> {
    doc.get("schemaVersion")?.as_u64()?.try_into().ok()
}

/// Bring `doc` up to the current schema. Returns the document and the version it started at.
pub fn migrate(doc: Value) -> Result<(Value, u32), StoreError> {
    migrate_with(doc, SCHEMA_VERSION, MIGRATIONS)
}

pub fn migrate_with(
    mut doc: Value,
    target: u32,
    chain: &[(u32, MigrationFn)],
) -> Result<(Value, u32), StoreError> {
    let start = schema_version(&doc).ok_or_else(|| StoreError::Migration {
        from: 0,
        reason: Message::MissingField {
            name: "schemaVersion".into(),
        },
    })?;
    if start > target {
        return Err(StoreError::UnsupportedVersion {
            found: start,
            supported: target,
        });
    }
    let mut version = start;
    while version < target {
        let step = chain
            .iter()
            .find(|(from, _)| *from == version)
            .ok_or(StoreError::Migration {
                from: version,
                reason: Message::NoMigration,
            })?
            .1;
        doc = step(doc).map_err(|message| StoreError::Migration {
            from: version,
            reason: Message::text(message),
        })?;
        version += 1;
        doc["schemaVersion"] = Value::from(version);
    }
    Ok((doc, start))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn rename_hosts(mut doc: Value) -> Result<Value, String> {
        for s in doc["servers"].as_array_mut().ok_or("servers 不是数组")? {
            let addr = s["address"].take();
            s["host"] = addr;
        }
        Ok(doc)
    }

    fn add_tags(mut doc: Value) -> Result<Value, String> {
        for s in doc["servers"].as_array_mut().ok_or("servers 不是数组")? {
            s["tags"] = json!([]);
        }
        Ok(doc)
    }

    #[test]
    fn current_version_is_unchanged() {
        let doc = json!({ "schemaVersion": SCHEMA_VERSION, "x": 1 });
        assert_eq!(migrate(doc.clone()).unwrap(), (doc, SCHEMA_VERSION));
    }

    #[test]
    fn chain_runs_in_order_and_bumps_the_version() {
        let chain: &[(u32, MigrationFn)] = &[(2, add_tags), (1, rename_hosts)];
        let doc = json!({ "schemaVersion": 1, "servers": [{ "address": "h" }] });
        let (out, from) = migrate_with(doc, 3, chain).unwrap();
        assert_eq!(from, 1);
        assert_eq!(
            out,
            json!({ "schemaVersion": 3, "servers": [{ "address": null, "host": "h", "tags": [] }] })
        );
    }

    #[test]
    fn newer_or_missing_versions_are_refused() {
        assert!(matches!(
            migrate(json!({ "schemaVersion": SCHEMA_VERSION + 1 })),
            Err(StoreError::UnsupportedVersion { .. })
        ));
        assert!(matches!(
            migrate(json!({})),
            Err(StoreError::Migration { .. })
        ));
        assert!(matches!(
            migrate_with(json!({ "schemaVersion": 1 }), 2, &[]),
            Err(StoreError::Migration { from: 1, .. })
        ));
    }

    #[test]
    fn failing_step_reports_its_version() {
        let chain: &[(u32, MigrationFn)] = &[(1, |_| Err("boom".into()))];
        assert!(matches!(
            migrate_with(json!({ "schemaVersion": 1 }), 2, chain),
            Err(StoreError::Migration { from: 1, reason }) if reason == Message::text("boom")
        ));
    }
}
