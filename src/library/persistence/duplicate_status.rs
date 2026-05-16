use super::transactions::*;
use super::*;

pub fn list_pattern_relations(path: &Path) -> Result<Vec<PatternRelation>, Td3Error> {
    let conn = open_partial_connection(path)?;
    load_json_rows(
        &conn,
        "SELECT json FROM pattern_relations ORDER BY position",
    )
}
