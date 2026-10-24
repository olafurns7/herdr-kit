use super::*;
use serde::Serialize;

/// Canonical typed JSON: recursively sorted object keys, compact UTF-8, array order retained.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<String> {
    fn append(value: &Value, out: &mut String) -> Result<()> {
        match value {
            Value::Object(map) => {
                out.push('{');
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                for (i, k) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(k)?);
                    out.push(':');
                    append(&map[k], out)?;
                }
                out.push('}');
            }
            Value::Array(a) => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    append(v, out)?;
                }
                out.push(']');
            }
            _ => out.push_str(&serde_json::to_string(value)?),
        }
        Ok(())
    }
    let mut out = String::new();
    append(&serde_json::to_value(value)?, &mut out)?;
    Ok(out)
}
fn profile_content(profile: &Profile) -> Result<Value> {
    let mut value = serde_json::to_value(profile)?;
    value.as_object_mut().unwrap().remove("digest");
    Ok(value)
}
/// SHA-256 of canonical {algorithm:"taskr-work-profile-v1",profile:<typed fields except digest>}.
pub fn profile_digest(profile: &Profile) -> Result<String> {
    Ok(digest(&canonical_json(
        &json!({"algorithm":"taskr-work-profile-v1","profile":profile_content(profile)?}),
    )?))
}
/// Commit-independent SHA-256 over approved typed profile and sorted pinned dependency inputs.
pub fn input_digest(profile: &Profile, dependencies: &[DependencyInput]) -> Result<String> {
    if dependencies.len() > 64 {
        return Err(usage("too many dependency inputs"));
    }
    let mut deps = dependencies.to_vec();
    deps.sort_by_key(|d| d.work);
    for (i, d) in deps.iter().enumerate() {
        hash(&d.input_digest, &[64])?;
        if d.work <= 0 || d.revision <= 0 || i > 0 && deps[i - 1].work == d.work {
            return Err(usage("invalid dependency inputs"));
        }
    }
    Ok(digest(&canonical_json(
        &json!({"algorithm":"taskr-work-input-v1","profile":profile_content(profile)?,"dependencies":deps}),
    )?))
}
pub(super) fn dependency_inputs(db: &Connection, c: &Contract) -> Result<Vec<DependencyInput>> {
    c.dependencies
        .iter()
        .map(|d| {
            let c = contract_revision(db, d.work, d.revision)?;
            Ok(DependencyInput {
                work: d.work,
                revision: d.revision,
                input_digest: c.base.input_digest,
            })
        })
        .collect()
}
pub fn inputs(db: &Connection, id: i64) -> Result<Value> {
    let w = item(db, id)?;
    let c = contract(db, &w)?;
    let deps = dependency_inputs(db, &c)?;
    Ok(
        json!({"work_id":id,"campaign":w.root,"revision":w.revision,"checkout":c.checkout,"base":c.base,"profile":c.profile,"dependency_inputs":deps,"input_digest":input_digest(&c.profile,&deps)?,"current_run":w.run,"dependency_receipts":dependencies(db,&c).ok().map(|s|serde_json::from_str::<Value>(&s)).transpose()?}),
    )
}
