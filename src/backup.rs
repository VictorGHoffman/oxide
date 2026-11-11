//! One file with everything: collections (and their variables) and global variables.

use serde::{Deserialize, Serialize};

use crate::model::{KeyValue, Tree};

const APP: &str = "oxide";
const KIND: &str = "backup";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Backup {
    app: String,
    kind: String,
    version: u32,
    /// Unix time (seconds) of the export.
    pub exported_at: u64,
    pub tree: Tree,
    pub globals: Vec<KeyValue>,
}

pub fn export(tree: &Tree, globals: &[KeyValue]) -> Result<String, String> {
    let backup = Backup {
        app: APP.into(),
        kind: KIND.into(),
        version: VERSION,
        exported_at: crate::oauth::now(),
        tree: tree.clone(),
        globals: globals.to_vec(),
    };
    serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())
}

pub fn parse(text: &str) -> Result<Backup, String> {
    let text = text.trim_start_matches('\u{feff}');
    let backup: Backup = serde_json::from_str(text)
        .map_err(|_| "This is not an Oxide backup file.".to_string())?;
    if backup.app != APP || backup.kind != KIND {
        return Err("This is not an Oxide backup file.".into());
    }
    if backup.version > VERSION {
        return Err("This backup was made by a newer version of Oxide.".into());
    }
    Ok(backup)
}

/// `2026-11-11` (UTC) for a Unix time.
pub fn date(secs: u64) -> String {
    // Civil-from-days (Howard Hinnant's algorithm).
    let days = (secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut tree = Tree::new();
        let id = tree.add_collection();
        tree.add_request(id);
        let globals = vec![KeyValue {
            enabled: true,
            key: "host".into(),
            value: "x.test".into(),
        }];
        let text = export(&tree, &globals).unwrap();
        let back = parse(&text).unwrap();
        assert_eq!(back.tree.roots.len(), 1);
        assert_eq!(back.globals[0].key, "host");

        assert!(parse("{}").is_err());
        assert!(parse("[1,2]").is_err());
        assert!(parse(&text.replace("\"backup\"", "\"other\"")).is_err());
    }

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_783_728_000), "2026-07-11");
        assert_eq!(date(1_762_819_200), "2025-11-11");
        assert_eq!(date(951_782_400), "2000-02-29");
    }
}
