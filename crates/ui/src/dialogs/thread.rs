//! Thread's own rows, drawn as Fusion draws them: Threads Library (its cloud button), then the
//! Thread Type, Size, Designation and Class drop-downs fed by the thread tables, and the status
//! bar's "1 Face | Radius: …" while a face is picked.

use fox_engine::Session;
use fox_engine::model::BodyId;
use serde_json::{Value, json};

use super::Dialog;
use crate::forms::Form;
use crate::icons::Icon;
use crate::theme;
use crate::thread_form::{self, TYPES};

impl Dialog {
  /// Draws Thread's input `i` when it is one of its table drop-downs; true when drawn here.
  pub(super) fn thread_form_field(
    &mut self,
    f: &'static Form,
    i: usize,
    s: &Session,
    ui: &mut egui::Ui,
    dirty: &mut bool,
  ) -> bool {
    let key = f.inputs[i].key();
    let get = |st: &crate::forms::State, k: &str| {
      f.inputs.iter().position(|x| x.key() == k).and_then(|j| st.values[j].as_str()).unwrap_or("").to_string()
    };
    let kind = get(&self.form, "thread_type");
    let internal = thread_form::internal_class(&get(&self.form, "class"));
    match key {
      "thread_type" => {
        // the cloud library of custom thread definitions isn't in FissionOxide
        ui.label("Threads Library");
        let why = "The cloud threads library isn't in FissionOxide yet";
        ui.add_enabled_ui(false, |ui| {
          crate::icons::icon_button(ui, Icon::ThreadsLibrary, 24.0, why, false, Some(theme::MUTED))
        })
        .inner
        .on_disabled_hover_text(why);
        ui.end_row();
        ui.label("Thread Type");
        let mut cur = kind.clone();
        let shown = TYPES.iter().find(|(k, _)| *k == cur).map(|(_, l)| *l).unwrap_or("");
        egui::ComboBox::from_id_salt(("thread-type", i)).selected_text(shown).width(180.0).show_ui(ui, |ui| {
          for (k, l) in TYPES {
            ui.selectable_value(&mut cur, k.to_string(), *l);
          }
        });
        ui.end_row();
        if cur != kind {
          self.form.values[i] = json!(cur);
          // the new type's size for the picked face
          let face = self.thread_face(f, s);
          let (d, inside) = face.map(|x| (x.0, x.1)).unwrap_or((if cur == "ansi" { 25.4 } else { 10.0 }, internal));
          thread_form::refit(f, &mut self.form, &cur, d, inside);
          *dirty = true;
        }
      }
      "size" => {
        let sizes = thread_form::sizes(&kind);
        let cur = get(&self.form, "size");
        let mut pick = cur.clone();
        let shown = sizes.iter().find(|x| x.key == cur).map(|x| x.label.clone()).unwrap_or_default();
        ui.label("Size");
        egui::ComboBox::from_id_salt(("thread-size", i)).selected_text(shown).width(180.0).show_ui(ui, |ui| {
          for x in &sizes {
            ui.selectable_value(&mut pick, x.key.clone(), &x.label);
          }
        });
        ui.end_row();
        if pick != cur {
          self.form.values[i] = json!(pick);
          // its coarse designation
          if let (Some(x), Some(j)) =
            (sizes.iter().find(|x| x.key == pick), f.inputs.iter().position(|x| x.key() == "designation"))
          {
            self.form.values[j] = json!(x.designations.first().map(|d| d.0.clone()).unwrap_or_default());
          }
          *dirty = true;
        }
      }
      "designation" => {
        let size = get(&self.form, "size");
        let list: Vec<String> = thread_form::sizes(&kind)
          .into_iter()
          .find(|x| x.key == size)
          .map(|x| x.designations.into_iter().map(|d| d.0).collect())
          .unwrap_or_default();
        self.thread_combo(ui, "Designation", i, &list, dirty);
      }
      "class" => {
        let list: Vec<String> = thread_form::classes(&kind, internal).iter().map(|c| c.to_string()).collect();
        self.thread_combo(ui, "Class", i, &list, dirty);
      }
      _ => return false,
    }
    true
  }

  /// A drop-down of plain values for input `i`.
  fn thread_combo(&mut self, ui: &mut egui::Ui, label: &str, i: usize, list: &[String], dirty: &mut bool) {
    let cur = self.form.values[i].as_str().unwrap_or("").to_string();
    let mut pick = cur.clone();
    ui.label(label);
    egui::ComboBox::from_id_salt(("thread-combo", i)).selected_text(cur.as_str()).width(180.0).show_ui(ui, |ui| {
      for x in list {
        ui.selectable_value(&mut pick, x.clone(), x);
      }
    });
    ui.end_row();
    if pick != cur {
      self.form.values[i] = json!(pick);
      *dirty = true;
    }
  }

  /// The first picked face's diameter (mm), whether it is a hole and its length.
  fn thread_face(&self, f: &Form, s: &Session) -> Option<(f64, bool, f64)> {
    let j = f.inputs.iter().position(|x| x.key() == "faces")?;
    let face = self.form.values[j].as_array()?.first()?.clone();
    thread_form::face_cylinder(s, &face)
  }

  /// "1 Face | Radius: 0.496 in" for the status bar while Thread has one face picked, as Fusion
  /// reads it out.
  pub(super) fn thread_status(&self, s: &Session) -> Option<String> {
    let super::Kind::Form(f) = self.kind else { return None };
    if f.cmd != "thread" {
      return None;
    }
    let j = f.inputs.iter().position(|x| x.key() == "faces")?;
    let faces = self.form.values[j].as_array()?;
    let n = faces.len();
    if n != 1 {
      return (n > 1).then(|| format!("{n} Faces"));
    }
    let v: &Value = &faces[0];
    let body = s.model.body(BodyId(v.get("body")?.as_u64()? as u32))?;
    let id = v.get("face")?.as_u64()? as u32;
    let r = body.faces().iter().find(|x| x.id == id)?.radius;
    let u = s.design.units;
    Some(format!("1 Face | Radius: {}", u.fixed(r, crate::viewport::display_decimals(u))))
  }
}
