//! Timeline groups: Fusion's Create Group on adjacent timeline items. A group collapses its
//! features into one timeline icon; it is only a way of showing the timeline, so it never changes
//! what the features build. Suppress and Delete on a group act on all of its features.

use serde::{Deserialize, Serialize};

use crate::{Design, FeatureId};

/// A named run of adjacent timeline features.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineGroup {
  pub id: u32,
  pub name: String,
  /// The grouped features, in timeline order.
  pub features: Vec<FeatureId>,
}

impl Design {
  pub fn group(&self, id: u32) -> Option<&TimelineGroup> {
    self.timeline_groups.iter().find(|g| g.id == id)
  }

  /// The group a feature is in.
  pub fn group_of(&self, f: FeatureId) -> Option<&TimelineGroup> {
    self.timeline_groups.iter().find(|g| g.features.contains(&f))
  }

  /// Timeline positions of `features`, sorted; an error unless they all exist and sit side by side.
  fn adjacent(&self, features: &[FeatureId]) -> Result<Vec<usize>, String> {
    let mut at: Vec<usize> =
      features.iter().map(|f| self.index_of(*f).ok_or(format!("no feature {}", f.0))).collect::<Result<_, _>>()?;
    at.sort_unstable();
    at.dedup();
    if at.is_empty() {
      return Err("nothing to group".into());
    }
    if at.windows(2).any(|w| w[1] != w[0] + 1) {
      return Err("only adjacent timeline items can be grouped".into());
    }
    Ok(at)
  }

  /// Whether Create Group can group `features`: they exist, sit side by side and are in no group.
  /// Returns their timeline positions.
  pub fn check_group(&self, features: &[FeatureId]) -> Result<Vec<usize>, String> {
    let at = self.adjacent(features)?;
    if let Some(g) = at.iter().find_map(|i| self.group_of(self.features[*i].id)) {
      return Err(format!("already in {}", g.name));
    }
    Ok(at)
  }

  /// Create Group: the features (adjacent in the timeline, in no group yet) become one group,
  /// named `name` or the next free "GroupN". Returns the group's id.
  pub fn create_group(&mut self, features: &[FeatureId], name: Option<&str>) -> Result<u32, String> {
    self.tidy_groups();
    let at = self.check_group(features)?;
    let id = self.timeline_groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
    let name = match name {
      Some(n) if !n.trim().is_empty() => n.trim().to_string(),
      // one of the first len + 1 names is always free
      _ => (1..=self.timeline_groups.len() + 1)
        .map(|k| format!("Group{k}"))
        .find(|n| !self.timeline_groups.iter().any(|g| g.name == *n))
        .unwrap_or_else(|| format!("Group{id}")),
    };
    let features = at.iter().map(|i| self.features[*i].id).collect();
    self.timeline_groups.push(TimelineGroup { id, name, features });
    Ok(id)
  }

  /// Ungroup: the group goes, its features stay where they are.
  pub fn ungroup(&mut self, id: u32) -> Result<(), String> {
    let n = self.timeline_groups.len();
    self.timeline_groups.retain(|g| g.id != id);
    if self.timeline_groups.len() == n { Err(format!("no group {id}")) } else { Ok(()) }
  }

  pub fn rename_group(&mut self, id: u32, name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
      return Err("a group needs a name".into());
    }
    let g = self.timeline_groups.iter_mut().find(|g| g.id == id).ok_or(format!("no group {id}"))?;
    g.name = name.trim().to_string();
    Ok(())
  }

  /// Move to Group: features next to the group (in no other group) join it.
  pub fn move_to_group(&mut self, id: u32, features: &[FeatureId]) -> Result<(), String> {
    self.tidy_groups();
    let g = self.group(id).ok_or(format!("no group {id}"))?;
    if let Some(other) = features.iter().find_map(|f| self.group_of(*f).filter(|o| o.id != id)) {
      return Err(format!("already in {}", other.name));
    }
    let mut all = g.features.clone();
    all.extend(features.iter().copied());
    let at = self.adjacent(&all).map_err(|_| "only items next to the group can move into it".to_string())?;
    let members = at.iter().map(|i| self.features[*i].id).collect();
    if let Some(g) = self.timeline_groups.iter_mut().find(|g| g.id == id) {
      g.features = members;
    }
    Ok(())
  }

  /// Drops deleted features from groups (and groups left empty) and keeps members in timeline
  /// order. Changes nothing a group-free design has.
  pub fn tidy_groups(&mut self) {
    if self.timeline_groups.is_empty() {
      return;
    }
    let order: std::collections::HashMap<FeatureId, usize> =
      self.features.iter().enumerate().map(|(i, f)| (f.id, i)).collect();
    for g in &mut self.timeline_groups {
      g.features.retain(|f| order.contains_key(f));
      g.features.sort_by_key(|f| order[f]);
    }
    self.timeline_groups.retain(|g| !g.features.is_empty());
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::FeatureKind;

  fn design(n: usize) -> (Design, Vec<FeatureId>) {
    let mut d = Design::new("g");
    let ids = (0..n)
      .map(|_| d.add_feature(FeatureKind::BaseFeature { bodies: Vec::new(), edits: Vec::new() }).unwrap())
      .collect();
    (d, ids)
  }

  #[test]
  fn groups_need_adjacent_ungrouped_items() {
    let (mut d, f) = design(4);
    assert!(d.create_group(&[f[0], f[2]], None).is_err());
    let g = d.create_group(&[f[2], f[1]], None).unwrap();
    assert_eq!(d.group(g).unwrap().name, "Group1");
    assert_eq!(d.group(g).unwrap().features, [f[1], f[2]]);
    assert!(d.create_group(&[f[2], f[3]], None).is_err(), "f2 is already grouped");
    // Move to Group: only a neighbour can join
    d.move_to_group(g, &[f[3]]).unwrap();
    assert_eq!(d.group(g).unwrap().features, [f[1], f[2], f[3]]);
    d.rename_group(g, "Holes").unwrap();
    assert_eq!(d.group_of(f[3]).unwrap().name, "Holes");
    d.ungroup(g).unwrap();
    assert!(d.timeline_groups.is_empty() && d.features.len() == 4);
  }
}
