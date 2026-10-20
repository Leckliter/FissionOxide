//! The undo history by name, for the Undo and Redo drop-downs on the quick-access toolbar: each
//! step is named from what it changed in the design ("Extrude1", "Edit Sketch1", "Delete Fillet2"),
//! and several steps can be undone or redone at once.

use fox_model::Design;

use crate::Session;

impl Session {
  /// The steps Undo goes back through, newest first, by name.
  pub fn undo_history(&self) -> Vec<String> {
    let start = self.txn.unwrap_or(0).min(self.undo.len());
    let stack = &self.undo[start..];
    (0..stack.len())
      .rev()
      .map(|i| {
        let after = stack.get(i + 1).unwrap_or(&self.design);
        step_label(&stack[i], after)
      })
      .collect()
  }

  /// The steps Redo goes forward through, next first, by name.
  pub fn redo_history(&self) -> Vec<String> {
    let mut before = &self.design;
    let mut out = Vec::with_capacity(self.redo.len());
    for d in self.redo.iter().rev() {
      out.push(step_label(before, d));
      before = d;
    }
    out
  }

  /// Undoes `n` steps (regenerating once). Returns how many were undone.
  pub fn undo_steps(&mut self, n: usize) -> usize {
    let mut done = 0;
    while done < n && self.can_undo() {
      let Some(d) = self.undo.pop() else { break };
      self.redo.push(std::mem::replace(&mut self.design, d));
      done += 1;
    }
    if done > 0 {
      self.after_history();
    }
    done
  }

  /// Redoes `n` steps (regenerating once). Returns how many were redone.
  pub fn redo_steps(&mut self, n: usize) -> usize {
    let mut done = 0;
    while done < n {
      let Some(d) = self.redo.pop() else { break };
      self.undo.push(std::mem::replace(&mut self.design, d));
      done += 1;
    }
    if done > 0 {
      self.after_history();
    }
    done
  }
}

/// What a step from `before` to `after` did, as the Undo list names it.
pub fn step_label(before: &Design, after: &Design) -> String {
  let had = |id| before.features.iter().any(|f| f.id == id);
  let has = |id| after.features.iter().any(|f| f.id == id);
  let added: Vec<&str> = after.features.iter().filter(|f| !had(f.id)).map(|f| f.name.as_str()).collect();
  if let Some(first) = added.first() {
    return match added.len() {
      1 => first.to_string(),
      n => format!("{first} (+{} more)", n - 1),
    };
  }
  if let Some(f) = before.features.iter().find(|f| !has(f.id)) {
    return format!("Delete {}", f.name);
  }
  if before.marker != after.marker {
    return "Move Timeline Marker".into();
  }
  if before.history_off != after.history_off {
    return if after.history_off { "Do Not Capture Design History" } else { "Capture Design History" }.into();
  }
  if before.parameters != after.parameters {
    let changed = after.parameters.iter().find(|p| !before.parameters.contains(p));
    return match changed {
      Some(p) if before.parameters.len() == after.parameters.len() => format!("Change Parameter {}", p.name),
      Some(p) => format!("Add Parameter {}", p.name),
      None => "Delete Parameter".into(),
    };
  }
  // features in timeline order: the first changed is the edited one (later ones only follow it)
  for (b, a) in before.features.iter().zip(&after.features) {
    if b.id != a.id {
      return format!("Reorder {}", a.name);
    }
    if b != a {
      return if b.name != a.name && b.kind == a.kind {
        format!("Rename {} to {}", b.name, a.name)
      } else if b.suppressed != a.suppressed && b.kind == a.kind {
        format!("{} {}", if a.suppressed { "Suppress" } else { "Unsuppress" }, a.name)
      } else if b.visible != a.visible && b.kind == a.kind {
        format!("{} {}", if a.visible == Some(false) { "Hide" } else { "Show" }, a.name)
      } else {
        format!("Edit {}", a.name)
      };
    }
  }
  let parts: [(bool, &str); 8] = [
    (before.bodies != after.bodies, "Change Body Display"),
    (before.components != after.components, "Edit Components"),
    (before.drawings != after.drawings, "Edit Drawing"),
    (before.configurations != after.configurations, "Edit Configurations"),
    (before.cam != after.cam, "Edit Manufacture Setup"),
    (before.sheet_metal_rules != after.sheet_metal_rules, "Edit Sheet Metal Rules"),
    (before.contact_sets != after.contact_sets, "Edit Contact Sets"),
    (before.name != after.name, "Rename Design"),
  ];
  parts.iter().find(|(changed, _)| *changed).map_or("Edit", |(_, l)| l).to_string()
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use crate::Session;

  fn session() -> Session {
    let mut s = Session::default();
    s.execute("box", &json!({"width": 10, "depth": 10, "height": 10})).unwrap();
    s.execute("param.add", &json!({"name": "w", "expr": "5 mm"})).unwrap();
    s
  }

  #[test]
  fn steps_are_named_newest_first() {
    let mut s = session();
    let names = s.undo_history();
    assert_eq!(names.len(), 2, "{names:?}");
    assert_eq!(names[0], "Add Parameter w");
    assert!(!names[1].starts_with("Edit") && !names[1].is_empty(), "the box is named: {names:?}");
    assert!(s.redo_history().is_empty());
    assert!(s.undo());
    assert_eq!(s.redo_history(), ["Add Parameter w"]);
    assert_eq!(s.undo_history().len(), 1);
  }

  #[test]
  fn jumping_back_and_forward_several_steps() {
    let mut s = session();
    let full = s.design.clone();
    let names = s.undo_history();
    assert_eq!(s.undo_steps(5), 2, "only two steps exist");
    assert!(s.design.features.is_empty());
    assert!(!s.can_undo());
    // redo lists the same steps, oldest first
    let mut redo = s.redo_history();
    redo.reverse();
    assert_eq!(redo, names);
    assert_eq!(s.redo_steps(2), 2);
    assert_eq!(s.design, full);
    assert_eq!(s.undo_history(), names);
  }
}
