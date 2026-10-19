//! Dialogs for the assembly commands added after joints: joint origins, rigid groups, motion links
//! and contact sets (see [`crate::forms`]).

use crate::forms::{Form, Input};
use crate::icons::Icon;

const MOTIONS: &[(&str, &str)] =
  &[("", "Joint's first"), ("rotation", "Rotation"), ("slide", "Slide"), ("slide2", "Slide 2"), ("pitch", "Pitch")];

pub static JOINT_ORIGIN: Form = Form {
  title: "JOINT ORIGIN",
  name: "Joint Origin",
  hint: "Save a joint origin on a component's face; joints can mate to it and it moves with the component",
  icon: Icon::JointOrigin,
  cmd: "joint_origin",
  fixed: &[],
  inputs: &[
    Input::Face { key: "face", label: "Face", near: None },
    Input::Expr { key: "angle", label: "Angle", default: "0 deg" },
    Input::Expr { key: "offset", label: "Offset", default: "0 mm" },
    Input::Check { key: "flip", label: "Flip" },
    Input::Text { key: "name", label: "Name", default: "" },
  ],
  picks_model: false,
};

pub static RIGID_GROUP: Form = Form {
  title: "RIGID GROUP",
  name: "Rigid Group",
  hint: "Make the selected bodies' components move as one",
  icon: Icon::Component,
  cmd: "rigid_group",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Components", required: true },
    Input::Text { key: "name", label: "Name", default: "" },
  ],
  picks_model: false,
};

pub static MOTION_LINK: Form = Form {
  title: "MOTION LINK",
  name: "Motion Link",
  hint: "Drive one joint from another (gears, rack and pinion): name the joints as in the browser",
  icon: Icon::DriveJoint,
  cmd: "motion_link",
  fixed: &[],
  inputs: &[
    Input::Text { key: "leader", label: "Joint 1 (drives)", default: "Joint1" },
    Input::Choice { key: "leader_motion", label: "Motion 1", options: MOTIONS },
    Input::Expr { key: "leader_value", label: "Value 1 (empty = 360° / 10 mm)", default: "" },
    Input::Text { key: "follower", label: "Joint 2 (driven)", default: "Joint2" },
    Input::Choice { key: "follower_motion", label: "Motion 2", options: MOTIONS },
    Input::Expr { key: "follower_value", label: "Value 2 (empty = 360° / 10 mm)", default: "" },
    Input::Check { key: "reverse", label: "Reverse" },
  ],
  picks_model: false,
};

pub static CONTACT_SET: Form = Form {
  title: "CONTACT SET",
  name: "Contact Set",
  hint: "The selected bodies' components can't pass through each other: driving a joint stops at contact",
  icon: Icon::Interference,
  cmd: "contact_set.create",
  fixed: &[],
  inputs: &[
    Input::Bodies { key: "bodies", label: "Components", required: true },
    Input::Text { key: "name", label: "Name", default: "" },
  ],
  picks_model: false,
};

#[cfg(test)]
mod tests {
  use clf_engine::Session;
  use serde_json::{Value, json};

  use super::*;
  use crate::forms::{State, engine_command};

  fn run_form(s: &mut Session, form: &Form, set: &[(&str, Value)]) -> Value {
    let mut st = State::new(form);
    for (k, v) in set {
      let i = form.inputs.iter().position(|x| x.key() == *k).unwrap();
      st.values[i] = v.clone();
    }
    assert_eq!(st.missing(form), None);
    let (cmd, p) = engine_command(form, st.params(form));
    let r = s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd} {p}: {}", e.0));
    assert!(r.get("error").is_none_or(Value::is_null), "{cmd} {p}: {r}");
    r
  }

  #[test]
  fn assembly_forms_run_their_commands() {
    let mut s = Session::default();
    let mut bodies = Vec::new();
    for (i, x) in [0, 50, 100].into_iter().enumerate() {
      s.execute("box", &json!({"width": 10, "depth": 10, "height": 10, "at": [x, 0]})).unwrap();
      let b = s.model.bodies[i].id.0;
      s.execute("component.create", &json!({"bodies": [b]})).unwrap();
      bodies.push(b);
    }
    let face = s.model.bodies[0].faces().iter().find(|f| f.normal.z > 0.9).unwrap().id;
    run_form(&mut s, &JOINT_ORIGIN, &[("face", json!({"body": bodies[0], "face": face})), ("name", json!("Top"))]);
    run_form(&mut s, &RIGID_GROUP, &[("bodies", json!([bodies[0], bodies[1]]))]);
    run_form(&mut s, &CONTACT_SET, &[("bodies", json!([bodies[1], bodies[2]]))]);
    assert_eq!(s.design.contact_sets.len(), 1);
    for (b, z) in [(bodies[1], [0, 0, 1]), (bodies[2], [1, 0, 0])] {
      let c = s.design.component_of(clf_engine::model::BodyId(b)).unwrap().id.0;
      let j = json!({"moving": c, "moving_origin": {"at": [0, 0, 0]}, "fixed_origin": {"at": [0, 0, 0], "z": z}, "kind": "slider", "as_built": true});
      s.execute("joint", &j).unwrap();
    }
    // default motions and values: slide to slide, 10 mm to 10 mm (C moves from x = 95 to 100)
    run_form(&mut s, &MOTION_LINK, &[("leader", json!("Joint1")), ("follower", json!("Joint2"))]);
    s.execute(
      "joint.drive",
      &json!({"joint": s.design.features.iter().find(|f| f.name == "Joint1").unwrap().id.0, "slide": 5}),
    )
    .unwrap();
    let x = s.model.bodies.iter().find(|b| b.id.0 == bodies[2]).unwrap().shape.mass().unwrap().bbox_min.x;
    assert!((x - 100.0).abs() < 1e-6, "{x}");
  }
}
