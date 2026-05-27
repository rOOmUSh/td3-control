use super::*;

pub(in crate::library::persistence) fn write_pattern_analysis(
    conn: &Connection,
    analyses: &[PatternAnalysis],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_PATTERN_ANALYSIS)?;
    let mut stmt = conn
        .prepare("INSERT INTO pattern_analysis (position, item_id, json) VALUES (?1, ?2, ?3)")
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: prepare sqlite pattern_analysis insert: {}",
                e
            ))
        })?;
    for (idx, analysis) in analyses.iter().enumerate() {
        stmt.execute(params![
            idx as i64,
            analysis.item_id.as_str(),
            to_json(analysis)?
        ])
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: insert sqlite pattern_analysis '{}': {}",
                analysis.item_id, e
            ))
        })?;
    }
    Ok(())
}

pub(in crate::library::persistence) fn write_pattern_relations(
    conn: &Connection,
    relations: &[PatternRelation],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_PATTERN_RELATIONS)?;
    let mut stmt = conn
        .prepare(
            "INSERT INTO pattern_relations (position, from_item_id, to_item_id, kind, json) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(|e| Td3Error::Other(format!("library: prepare sqlite pattern_relations insert: {}", e)))?;
    for (idx, relation) in relations.iter().enumerate() {
        stmt.execute(params![
            idx as i64,
            relation.from_item_id.as_str(),
            relation.to_item_id.as_str(),
            relation_kind_text(relation.kind),
            to_json(relation)?,
        ])
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: insert sqlite pattern_relation '{}->{}': {}",
                relation.from_item_id, relation.to_item_id, e
            ))
        })?;
    }
    Ok(())
}

fn relation_kind_text(kind: crate::library::model::RelationKind) -> &'static str {
    use crate::library::model::RelationKind;
    match kind {
        RelationKind::SameScale => "same_scale",
        RelationKind::SameRoot => "same_root",
        RelationKind::SameRhythm => "same_rhythm",
        RelationKind::NearDuplicate => "near_duplicate",
        RelationKind::AnalyzerRelated => "analyzer_related",
        RelationKind::ProgressionFamily => "progression_family",
    }
}
