//! Profile queries: conditions turned into bound SQL filters and a random-start sampled scan.

use super::*;
use dyapp_p2p_net::proto::{Condition, Op};
use dyapp_p2p_net::search_results::MAX_IN_VALUES;
use dyapp_profile::{
    scale, CareerAmbition, EmploymentStatus, FinancialPhilosophy, FitnessLevel, Gender,
    Orientation, RelationshipGoal, RolePreference, Zodiac,
};

/// Profile field number → column and whether it holds integers. Constant identifiers only.
const COLUMNS: &[(u32, &str, bool)] = &[
    (4, "country", false),
    (6, "income_from", true),
    (7, "income_to", true),
    (8, "has_kids", true),
    (9, "wants_more_kids", true),
    (RelationshipGoal::FIELD, "relationship_goal", true),
    (CareerAmbition::FIELD, "career_ambition", true),
    (Orientation::FIELD, "orientation", true),
    (RolePreference::FIELD, "role_preference", true),
    (Zodiac::FIELD, "zodiac", true),
    (FinancialPhilosophy::FIELD, "financial_philosophy", true),
    (EmploymentStatus::FIELD, "employment_status", true),
    (FitnessLevel::FIELD, "fitness_level", true),
    (20, "place", false),
    (21, "photo_count", true),
    (Gender::FIELD, "gender", true),
    (27, "education_level", true),
];
const INTERESTS: u32 = 19;
const ACTIVE_DAY: u32 = dyapp_p2p_net::search_results::ACTIVE_DAY;
/// Virtual condition: inclusive birth-date bounds at the query's UTC date.
pub const BIRTH_AGE: u32 = dyapp_p2p_net::search_results::BIRTH_AGE;
/// Profile field numbers a query may filter on, plus virtual fields.
pub const SEARCH_FIELDS: &[u32] = &[
    4,
    6,
    7,
    8,
    9,
    RelationshipGoal::FIELD,
    CareerAmbition::FIELD,
    Orientation::FIELD,
    RolePreference::FIELD,
    Zodiac::FIELD,
    FinancialPhilosophy::FIELD,
    EmploymentStatus::FIELD,
    FitnessLevel::FIELD,
    19,
    20,
    21,
    Gender::FIELD,
    27,
    ACTIVE_DAY,
    BIRTH_AGE,
];

/// Result of a query.
#[derive(Debug)]
pub struct Found {
    /// Matching keys with their indexed versions.
    pub hits: Vec<([u8; 32], u64)>,
    /// Fields of the conditions that were applied.
    pub applied: Vec<u32>,
    /// Whether the scan stopped at its row limit before `limit` hits.
    pub partial: bool,
}

/// A condition turned into SQL over the `p` alias, with its bound values.
struct Filter {
    field: u32,
    sql: String,
    values: Vec<rusqlite::types::Value>,
}

/// `Ok(None)`: an unknown field or op, or `range` on a string field — skipped.
/// `Err(())`: a malformed condition — the query is `INVALID`.
fn filter(c: &Condition) -> std::result::Result<Option<Filter>, ()> {
    use rusqlite::types::Value;
    let op = Op::try_from(c.op).unwrap_or(Op::Unspecified);
    if c.field == BIRTH_AGE {
        if op == Op::Unspecified {
            return Ok(None);
        }
        if op != Op::Range || c.ints.len() != 2 || !c.strings.is_empty() || c.ints[0] >= c.ints[1] {
            return Err(());
        }
        return Ok(Some(Filter {
            field: c.field,
            sql: "COALESCE(`p`.`birth_earliest` > ? AND `p`.`birth_latest` <= ?, 0)".into(),
            values: c.ints.iter().copied().map(Value::Integer).collect(),
        }));
    }
    let interests = c.field == INTERESTS;
    let active = c.field == ACTIVE_DAY;
    let Some(&(_, column, int)) = COLUMNS
        .iter()
        .find(|(f, ..)| *f == c.field)
        .or(interests.then_some(&(INTERESTS, "", false)))
        .or(active.then_some(&(ACTIVE_DAY, "", true)))
    else {
        return Ok(None);
    };
    let values: Vec<Value> = if int {
        if !c.strings.is_empty() {
            return Err(());
        }
        c.ints.iter().map(|v| Value::Integer(*v)).collect()
    } else {
        if !c.ints.is_empty() {
            return Err(());
        }
        c.strings.iter().map(|v| Value::Text(v.clone())).collect()
    };
    let n = values.len();
    if let Some(scale) = scale(c.field) {
        if (op == Op::Range && !scale.ordered)
            || (op != Op::Unspecified
                && c.ints
                    .iter()
                    .any(|&value| value < 1 || value > i64::from(scale.max)))
        {
            return Err(());
        }
    }
    let target = if interests {
        "EXISTS (SELECT 1 FROM `interest` AS `i` WHERE `i`.`key` = `p`.`key` AND `i`.`interest`"
            .to_string()
    } else if active {
        "EXISTS (SELECT 1 FROM `key` AS `k` WHERE `k`.`key` = `p`.`key` AND `k`.`heartbeat` / 86400"
            .to_string()
    } else {
        format!("`p`.`{column}`")
    };
    let close = if interests || active { ")" } else { "" };
    let sql = match op {
        Op::Unspecified => return Ok(None),
        Op::Range if !int => return Ok(None),
        Op::Eq if n == 1 => format!("{target} = ?{close}"),
        Op::In if (1..=MAX_IN_VALUES).contains(&n) => {
            format!("{target} IN ({}){close}", vec!["?"; n].join(", "))
        }
        Op::Range if n == 2 && c.ints[0] <= c.ints[1] => format!("{target} BETWEEN ? AND ?{close}"),
        _ => return Err(()),
    };
    Ok(Some(Filter {
        field: c.field,
        sql,
        values,
    }))
}

impl SearchIndex {
    /// A random sample of up to `limit` matching keys, reading at most `max_scan_rows` rows.
    /// The inner `Err(())` means the conditions are `INVALID`.
    pub fn query(
        &self,
        conditions: &[Condition],
        limit: u32,
        max_scan_rows: u32,
    ) -> Result<std::result::Result<Found, ()>> {
        use rusqlite::types::Value;
        if conditions.len() > 16 {
            return Ok(Err(()));
        }
        let mut seen = std::collections::HashSet::new();
        if !conditions.iter().all(|c| seen.insert(c.field)) {
            return Ok(Err(()));
        }
        let mut filters = Vec::new();
        for c in conditions {
            match filter(c) {
                Ok(Some(f)) => filters.push(f),
                Ok(None) => {}
                Err(()) => return Ok(Err(())),
            }
        }
        let applied = filters.iter().map(|f| f.field).collect();
        let limit = match limit {
            0 => 50,
            n => n.min(200),
        } as usize;
        // Scopes: (country, place) pairs on the index prefix; the rest filters inside the scan.
        // Two `in` filters on country and place make at most 256 scoped statements;
        // the scan-row budget caps rows, while preparation remains bounded separately.
        let take = |field: u32, filters: &mut Vec<Filter>| {
            filters
                .iter()
                .position(|f| f.field == field)
                .map(|i| filters.remove(i).values)
        };
        let countries = take(4, &mut filters);
        let places = if countries.is_some() {
            take(20, &mut filters)
        } else {
            None
        };
        let mut scopes: Vec<(&str, Vec<Value>)> = Vec::new();
        match (&countries, &places) {
            (Some(cs), Some(ps)) => {
                for c in cs {
                    for p in ps {
                        scopes.push((
                            "`p`.`country` = ? AND `p`.`place` = ?",
                            vec![c.clone(), p.clone()],
                        ));
                    }
                }
            }
            (Some(cs), None) => {
                for c in cs {
                    scopes.push(("`p`.`country` = ?", vec![c.clone()]));
                }
            }
            _ => scopes.push(("1", vec![])),
        }
        let rest = if filters.is_empty() {
            "1".to_string()
        } else {
            filters
                .iter()
                .map(|f| f.sql.as_str())
                .collect::<Vec<_>>()
                .join(" AND ")
        };
        let rest_values: Vec<Value> = filters.into_iter().flat_map(|f| f.values).collect();
        let db = self.reader()?;
        let s = i64::from_ne_bytes(rand_bytes());
        let (mut hits, mut scanned) = (Vec::new(), 0u32);
        // From `s` up, then wrap to below `s`.
        for (lo, hi) in [(s, i64::MAX), (i64::MIN, s.saturating_sub(1))] {
            if hits.len() >= limit || scanned >= max_scan_rows || lo > hi {
                break;
            }
            let budget = max_scan_rows - scanned;
            let mut statements = Vec::new();
            for (scope, _) in &scopes {
                statements.push(
                    db.prepare(&format!(
                        "SELECT `p`.`key`, `p`.`version`, `p`.`r`, COALESCE(({rest}), 0) FROM `profile` AS `p`
                     WHERE {scope} AND `p`.`r` BETWEEN ? AND ? ORDER BY `p`.`r` LIMIT ?"
                    ))
                    .map_err(storage_error)?,
                );
            }
            // The `({rest})` placeholders come first, then the scope, the range and LIMIT.
            let mut cursors = Vec::new();
            for (statement, (_, scope_values)) in statements.iter_mut().zip(&scopes) {
                let mut values = rest_values.clone();
                values.extend(scope_values.iter().cloned());
                values.extend([
                    Value::Integer(lo),
                    Value::Integer(hi),
                    Value::Integer(i64::from(budget)),
                ]);
                cursors.push(
                    statement
                        .query(rusqlite::params_from_iter(values))
                        .map_err(storage_error)?,
                );
            }
            let mut heads = Vec::new();
            for rows in &mut cursors {
                heads.push(next(rows)?);
            }
            // Merge the scopes by `r`.
            while hits.len() < limit && scanned < max_scan_rows {
                let Some(i) = (0..heads.len())
                    .filter(|i| heads[*i].is_some())
                    .min_by_key(|i| heads[*i].as_ref().map(|h| h.2))
                else {
                    break;
                };
                let (key, version, _, ok) = heads[i].take().expect("picked a live head");
                scanned += 1;
                if ok {
                    hits.push((key, version));
                }
                heads[i] = next(&mut cursors[i])?;
            }
        }
        // A refilling index misses keys: clients complete the results from other nodes.
        let partial = hits.len() < limit && scanned >= max_scan_rows || self.refilling();
        Ok(Ok(Found {
            hits,
            applied,
            partial,
        }))
    }
}

/// A scanned row: key, version, `r` and whether the other filters matched.
type Head = ([u8; 32], u64, i64, bool);

fn next(rows: &mut rusqlite::Rows<'_>) -> Result<Option<Head>> {
    let Some(row) = rows.next().map_err(storage_error)? else {
        return Ok(None);
    };
    let key: Vec<u8> = row.get(0).map_err(storage_error)?;
    Ok(Some((
        key.try_into().map_err(|_| storage_error("bad key"))?,
        row.get::<_, i64>(1).map_err(storage_error)?.unsigned_abs(),
        row.get(2).map_err(storage_error)?,
        row.get(3).map_err(storage_error)?,
    )))
}
