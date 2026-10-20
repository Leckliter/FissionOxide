//! Timeline groups (Fusion's Create Group / Move to Group / Ungroup on the timeline). A group is
//! only how the timeline shows its features, so grouping never changes what they build; Suppress
//! and Delete on a group act on every feature in it, as one undo step.

use fox_model::FeatureId;
use serde_json::{Value, json};

use crate::commands::{ids, opt_str, req_str, req_u32};
use crate::{CmdError, CmdResult, Session};

pub(crate) fn run(s: &mut Session, name: &str, p: &Value) -> Option<CmdResult> {
  Some(match name {
    "timeline.group" => (|| {
      let features = feature_ids(p)?;
      let name = opt_str(p, "name").map(str::to_string);
      let id = s.edit(|d| d.create_group(&features, name.as_deref()).map_err(CmdError::new))?;
      Ok(json!({"group": id}))
    })(),
    "timeline.ungroup" => (|| {
      let id = req_u32(p, "group")?;
      s.edit(|d| d.ungroup(id).map_err(CmdError::new))?;
      Ok(json!({}))
    })(),
    "timeline.group_rename" => (|| {
      let (id, n) = (req_u32(p, "group")?, req_str(p, "name")?.to_string());
      s.edit(|d| d.rename_group(id, &n).map_err(CmdError::new))?;
      Ok(json!({}))
    })(),
    "timeline.group_add" => (|| {
      let id = req_u32(p, "group")?;
      let features = feature_ids(p)?;
      s.edit(|d| d.move_to_group(id, &features).map_err(CmdError::new))?;
      Ok(json!({}))
    })(),
    "timeline.group_suppress" => (|| {
      let id = req_u32(p, "group")?;
      let on = p.get("suppressed").and_then(Value::as_bool);
      s.edit(|d| {
        let members = d.group(id).ok_or(CmdError::new(format!("no group {id}")))?.features.clone();
        // toggling: suppress all unless they all are already
        let on = on.unwrap_or_else(|| !members.iter().all(|f| d.feature(*f).is_some_and(|f| f.suppressed)));
        for f in members {
          if let Some(f) = d.feature_mut(f) {
            f.suppressed = on;
          }
        }
        Ok(())
      })?;
      Ok(json!({}))
    })(),
    "timeline.group_delete" => delete(s, p),
    _ => return None,
  })
}

fn feature_ids(p: &Value) -> Result<Vec<FeatureId>, CmdError> {
  let f = ids(p, "features");
  if f.is_empty() {
    return Err(CmdError::new("missing 'features'"));
  }
  Ok(f.into_iter().map(FeatureId).collect())
}

/// Delete on a group: its features (and what depends on them) go, in one undo step.
fn delete(s: &mut Session, p: &Value) -> CmdResult {
  let id = req_u32(p, "group")?;
  let members = s.design.group(id).ok_or(CmdError::new(format!("no group {id}")))?.features.clone();
  let own = !s.in_transaction();
  if own {
    s.begin_transaction()?;
  }
  let mut removed = Vec::new();
  let mut result = Ok(());
  // last first, so a later member never loses what it is built on before it goes
  for f in members.iter().rev() {
    if s.design.feature(*f).is_none() {
      continue;
    }
    match s.execute("feature.delete", &json!({"feature": f.0})) {
      Ok(v) => removed.extend(v["removed"].as_array().cloned().unwrap_or_default()),
      Err(e) => {
        result = Err(e);
        break;
      }
    }
  }
  // the group goes with its features (an empty group is dropped by tidy_groups)
  if result.is_ok() && s.design.group(id).is_some() {
    result = s.edit(|d| d.ungroup(id).map_err(CmdError::new));
  }
  if own {
    if result.is_err() {
      s.rollback_transaction()?;
    } else {
      s.commit_transaction()?;
    }
  }
  result?;
  Ok(json!({"removed": removed}))
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use crate::Session;

  fn volume(s: &Session) -> f64 {
    s.model.bodies.iter().map(|b| b.shape.mass().unwrap().volume).sum()
  }

  fn boxes() -> (Session, Vec<u32>) {
    let mut s = Session::default();
    for (i, h) in [10, 20, 30].iter().enumerate() {
      let at = json!([i as f64 * 50.0, 0]);
      s.execute("box", &json!({"width": 10, "depth": 10, "height": h, "at": at})).unwrap();
    }
    let ids = s.design.features.iter().map(|f| f.id.0).collect();
    (s, ids)
  }

  #[test]
  fn groups_round_trip_through_save_and_open() {
    let (mut s, f) = boxes();
    let (v, bodies) = (volume(&s), s.model.bodies.len());
    let g = s.execute("timeline.group", &json!({"features": [f[1], f[0]]})).unwrap()["group"].as_u64().unwrap();
    s.execute("timeline.group_rename", &json!({"group": g, "name": "Posts"})).unwrap();
    assert!(s.execute("timeline.group", &json!({"features": [f[1], f[2]]})).is_err(), "already grouped");
    // grouping never changes what the features build
    assert!((volume(&s) - v).abs() < 1e-9 && s.model.bodies.len() == bodies);
    assert_eq!(s.undo_history()[..2], ["Rename Group", "Create Group"]);
    let dir = std::env::temp_dir().join(format!("fox-groups-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("grouped.fox");
    s.save_to(&path).unwrap();
    let back = Session::open(&path).unwrap();
    assert_eq!(back.design.timeline_groups, s.design.timeline_groups);
    let group = back.design.group(g as u32).unwrap();
    assert_eq!(group.name, "Posts");
    assert_eq!(group.features.iter().map(|f| f.0).collect::<Vec<_>>(), [f[0], f[1]]);
    assert!((volume(&back) - v).abs() < 1e-9);
    // a design without groups saves no group field, so older files read and write unchanged
    let (mut plain, _) = boxes();
    let path2 = dir.join("plain.fox");
    plain.save_to(&path2).unwrap();
    let text = std::fs::read_to_string(&path2).unwrap();
    assert!(!text.contains("timeline_groups"), "{text}");
    assert!(Session::open(&path2).unwrap().design.timeline_groups.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn suppress_and_delete_act_on_the_whole_group() {
    let (mut s, f) = boxes();
    let g = s.execute("timeline.group", &json!({"features": [f[0], f[1]]})).unwrap()["group"].clone();
    // Move to Group: the neighbour joins
    s.execute("timeline.group_add", &json!({"group": g, "features": [f[2]]})).unwrap();
    s.execute("timeline.group_suppress", &json!({"group": g})).unwrap();
    assert!(s.design.features.iter().all(|f| f.suppressed));
    s.execute("timeline.group_suppress", &json!({"group": g})).unwrap();
    assert!(s.design.features.iter().all(|f| !f.suppressed));
    s.execute("timeline.group_delete", &json!({"group": g})).unwrap();
    assert!(s.design.features.is_empty() && s.design.timeline_groups.is_empty());
    assert_eq!(s.undo_history()[0], "Delete Group");
    // one step brings the group and its features back
    assert!(s.undo());
    assert_eq!(s.design.features.len(), 3);
    assert_eq!(s.design.timeline_groups.len(), 1);
    // Ungroup keeps the features
    s.execute("timeline.ungroup", &json!({"group": g})).unwrap();
    assert!(s.design.timeline_groups.is_empty() && s.design.features.len() == 3);
  }

  #[test]
  fn deleting_a_member_shrinks_its_group() {
    let (mut s, f) = boxes();
    s.execute("timeline.group", &json!({"features": [f[0], f[1]]})).unwrap();
    s.execute("feature.delete", &json!({"feature": f[1]})).unwrap();
    assert_eq!(s.design.timeline_groups[0].features.len(), 1);
    s.execute("feature.delete", &json!({"feature": f[0]})).unwrap();
    assert!(s.design.timeline_groups.is_empty());
  }
}
