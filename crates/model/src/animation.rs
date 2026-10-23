//! The Animation workspace's storyboards, saved with the design: actions over time that move,
//! hide, fade and label components and move the camera. Like an exploded view they are display
//! only: they never enter the timeline, move a component for joints or change what the design
//! shows outside the Animation workspace.
//!
//! A storyboard's [`Pose`] at a time is worked out by applying its actions in start order, each
//! as far as it has run: a transform turns and moves its bodies by that fraction, a show or hide
//! fades them in or out, an opacity change blends towards its value, a view glides the camera.

use std::collections::HashMap;

use fox_kernel::glam::{DAffine3, DQuat, DVec3, EulerRot};
use serde::{Deserialize, Serialize};

use crate::explode::subtree_bodies;
use crate::{BodyId, ComponentId, Design};

/// The length a new action gets (seconds).
pub const DEFAULT_DURATION: f64 = 1.0;

/// The storyboards of a design.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Animation {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub storyboards: Vec<Storyboard>,
  #[serde(default)]
  next_action: u32,
}

/// How a storyboard starts: from the assembled design, or where the one before it ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryboardKind {
  #[default]
  Clean,
  StartFromEnd,
}

/// One animation of the design: its own timeline of actions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Storyboard {
  pub name: String,
  #[serde(default)]
  pub kind: StoryboardKind,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub actions: Vec<Action>,
}

/// What an action acts on: components (with the components below them) and bodies of the root.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Targets {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub components: Vec<ComponentId>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub bodies: Vec<BodyId>,
}

impl Targets {
  pub fn is_empty(&self) -> bool {
    self.components.is_empty() && self.bodies.is_empty()
  }

  /// Every body acted on.
  pub fn bodies(&self, d: &Design) -> Vec<BodyId> {
    let mut out: Vec<BodyId> = Vec::new();
    for c in &self.components {
      for b in subtree_bodies(d, *c) {
        if !out.contains(&b) {
          out.push(b);
        }
      }
    }
    for b in &self.bodies {
      if !out.contains(b) {
        out.push(*b);
      }
    }
    out
  }
}

/// The command that recorded a transform (the timeline names its bar after it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformSource {
  #[default]
  Transform,
  RestoreHome,
  AutoExplode,
  ManualExplode,
}

/// A camera, as a view action records it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct AnimCamera {
  pub target: [f64; 3],
  pub yaw: f64,
  pub pitch: f64,
  pub distance: f64,
  pub fov: f64,
  #[serde(default)]
  pub ortho: bool,
}

impl AnimCamera {
  /// The camera `f` of the way from `self` to `to`; yaw turns the short way round.
  pub fn lerp(&self, to: &AnimCamera, f: f64) -> AnimCamera {
    let l = |a: f64, b: f64| a + (b - a) * f;
    let tau = std::f64::consts::TAU;
    let dyaw = (to.yaw - self.yaw + std::f64::consts::PI).rem_euclid(tau) - std::f64::consts::PI;
    AnimCamera {
      target: [l(self.target[0], to.target[0]), l(self.target[1], to.target[1]), l(self.target[2], to.target[2])],
      yaw: self.yaw + dyaw * f,
      pitch: l(self.pitch, to.pitch),
      // zoom evenly: the distance changes by the same factor each moment
      distance: (self.distance.max(1e-9).ln() + (to.distance.max(1e-9).ln() - self.distance.max(1e-9).ln()) * f).exp(),
      fov: l(self.fov, to.fov),
      ortho: if f < 1.0 { self.ortho } else { to.ortho },
    }
  }
}

/// What an action does.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
  /// Turns the bodies about `pivot` by `rotate` (degrees about X, then Y, then Z) and moves
  /// them by `translate` (mm).
  Transform {
    targets: Targets,
    #[serde(default)]
    translate: [f64; 3],
    #[serde(default)]
    rotate: [f64; 3],
    #[serde(default)]
    pivot: [f64; 3],
    #[serde(default)]
    source: TransformSource,
  },
  /// Shows or hides the bodies, fading over the action.
  Visibility { targets: Targets, show: bool },
  /// Blends the bodies' opacity (0 to 1) to `opacity`.
  Opacity { targets: Targets, opacity: f64 },
  /// A text callout on the model at `anchor`, moving with `body`; shown from the action's start.
  Callout {
    anchor: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body: Option<BodyId>,
    text: String,
  },
  /// Glides the camera from `from` to `to`.
  View { from: AnimCamera, to: AnimCamera },
}

impl ActionKind {
  /// The action's name on the timeline.
  pub fn label(&self) -> &'static str {
    match self {
      ActionKind::Transform { source, .. } => match source {
        TransformSource::Transform => "Transform",
        TransformSource::RestoreHome => "Restore Home",
        TransformSource::AutoExplode => "Auto Explode",
        TransformSource::ManualExplode => "Manual Explode",
      },
      ActionKind::Visibility { show: true, .. } => "Show",
      ActionKind::Visibility { show: false, .. } => "Hide",
      ActionKind::Opacity { .. } => "Opacity",
      ActionKind::Callout { .. } => "Callout",
      ActionKind::View { .. } => "View",
    }
  }

  pub fn targets(&self) -> Option<&Targets> {
    match self {
      ActionKind::Transform { targets, .. }
      | ActionKind::Visibility { targets, .. }
      | ActionKind::Opacity { targets, .. } => Some(targets),
      ActionKind::Callout { .. } | ActionKind::View { .. } => None,
    }
  }
}

/// A timed action of a storyboard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Action {
  pub id: u32,
  /// When it starts and how long it runs (seconds).
  pub start: f64,
  pub duration: f64,
  pub kind: ActionKind,
}

impl Action {
  pub fn end(&self) -> f64 {
    self.start + self.duration
  }

  /// How far it has run at `t`: 0 before its start, 1 from its end.
  pub fn fraction(&self, t: f64) -> f64 {
    if t < self.start {
      0.0
    } else if self.duration <= 1e-9 {
      1.0
    } else {
      ((t - self.start) / self.duration).clamp(0.0, 1.0)
    }
  }
}

/// The rotation of turning about X by `a[0]`, then Y by `a[1]`, then Z by `a[2]` (degrees).
pub fn rotation(a: [f64; 3]) -> DQuat {
  DQuat::from_euler(EulerRot::ZYX, a[2].to_radians(), a[1].to_radians(), a[0].to_radians())
}

/// The X, Y and Z angles (degrees, as [`rotation`] takes them) of a rotation.
pub fn angles(q: DQuat) -> [f64; 3] {
  let (z, y, x) = q.normalize().to_euler(EulerRot::ZYX);
  [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

/// The move of a transform `f` of the way through: turned about `pivot` and moved.
pub fn transform_at(translate: [f64; 3], rotate: [f64; 3], pivot: [f64; 3], f: f64) -> DAffine3 {
  let p = DVec3::from(pivot);
  let q = DQuat::IDENTITY.slerp(rotation(rotate), f);
  DAffine3::from_translation(p + DVec3::from(translate) * f) * DAffine3::from_quat(q) * DAffine3::from_translation(-p)
}

/// A callout shown at a time.
#[derive(Debug, Clone, PartialEq)]
pub struct ShownCallout {
  pub action: u32,
  /// Where it points, moved with its body.
  pub anchor: DVec3,
  pub text: String,
}

/// What a storyboard shows at a time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pose {
  /// Bodies moved from where the design has them.
  pub transforms: HashMap<BodyId, DAffine3>,
  /// Bodies shown or hidden other than as the design has them.
  pub visible: HashMap<BodyId, bool>,
  /// Bodies drawn see-through (0 to 1; fading in or out included).
  pub opacity: HashMap<BodyId, f64>,
  pub callouts: Vec<ShownCallout>,
  /// The camera, once a view action has started.
  pub camera: Option<AnimCamera>,
}

impl Pose {
  /// A body's move (identity when it hasn't moved).
  pub fn transform(&self, b: BodyId) -> DAffine3 {
    self.transforms.get(&b).copied().unwrap_or(DAffine3::IDENTITY)
  }

  /// Whether a body shows, given whether the design shows it.
  pub fn shows(&self, b: BodyId, design_shows: bool) -> bool {
    self.visible.get(&b).copied().unwrap_or(design_shows)
  }
}

impl Storyboard {
  pub fn new(name: impl Into<String>, kind: StoryboardKind) -> Storyboard {
    Storyboard { name: name.into(), kind, actions: Vec::new() }
  }

  /// Its length: the end of its last action (0 with none).
  pub fn length(&self) -> f64 {
    self.actions.iter().map(Action::end).fold(0.0, f64::max)
  }

  /// The actions in the order they apply: by start, then as recorded.
  fn ordered(&self) -> Vec<&Action> {
    let mut a: Vec<&Action> = self.actions.iter().collect();
    a.sort_by(|x, y| x.start.total_cmp(&y.start).then(x.id.cmp(&y.id)));
    a
  }

  pub fn action(&self, id: u32) -> Option<&Action> {
    self.actions.iter().find(|a| a.id == id)
  }
}

impl Animation {
  pub fn is_empty(&self) -> bool {
    self.storyboards.is_empty()
  }

  /// The storyboards as the timeline lists them: one ("Storyboard1") before any is made, as
  /// Fusion starts.
  pub fn listed(&self) -> Vec<Storyboard> {
    if self.storyboards.is_empty() {
      vec![Storyboard::new("Storyboard1", StoryboardKind::Clean)]
    } else {
      self.storyboards.clone()
    }
  }

  /// Storyboard `i`, making the first one if there is none yet.
  pub fn storyboard_mut(&mut self, i: usize) -> Option<&mut Storyboard> {
    if self.storyboards.is_empty() {
      self.storyboards.push(Storyboard::new("Storyboard1", StoryboardKind::Clean));
    }
    self.storyboards.get_mut(i)
  }

  /// The name the next storyboard gets: Storyboard1, Storyboard2, … past the highest in use.
  pub fn next_name(&self) -> String {
    let n =
      self.listed().iter().filter_map(|s| s.name.strip_prefix("Storyboard")?.parse::<usize>().ok()).max().unwrap_or(0);
    format!("Storyboard{}", n + 1)
  }

  /// Adds a storyboard (after the listed first one) and returns its index.
  pub fn add_storyboard(&mut self, name: Option<String>, kind: StoryboardKind) -> usize {
    let name = name.unwrap_or_else(|| self.next_name());
    if self.storyboards.is_empty() {
      self.storyboards.push(Storyboard::new("Storyboard1", StoryboardKind::Clean));
    }
    self.storyboards.push(Storyboard::new(name, kind));
    self.storyboards.len() - 1
  }

  /// Adds an action to storyboard `i`; returns its id.
  pub fn add_action(&mut self, i: usize, start: f64, duration: f64, kind: ActionKind) -> Option<u32> {
    self.next_action = self.next_action.max(self.max_id()) + 1;
    let id = self.next_action;
    let sb = self.storyboard_mut(i)?;
    sb.actions.push(Action { id, start: start.max(0.0), duration: duration.max(0.0), kind });
    Some(id)
  }

  fn max_id(&self) -> u32 {
    self.storyboards.iter().flat_map(|s| s.actions.iter().map(|a| a.id)).max().unwrap_or(0)
  }

  /// What storyboard `i` shows at `t`. `shown` says whether the design shows a body.
  pub fn pose(&self, d: &Design, i: usize, t: f64, shown: &dyn Fn(BodyId) -> bool) -> Pose {
    let mut pose = Pose::default();
    let Some(sb) = self.storyboards.get(i) else { return pose };
    if sb.kind == StoryboardKind::StartFromEnd && i > 0 {
      let prev = &self.storyboards[i - 1];
      pose = self.pose(d, i - 1, prev.length(), shown);
      // a storyboard's callouts stay with it
      pose.callouts.clear();
    }
    let mut callouts: Vec<&Action> = Vec::new();
    for a in sb.ordered() {
      if t < a.start {
        continue;
      }
      let f = a.fraction(t);
      match &a.kind {
        ActionKind::Transform { targets, translate, rotate, pivot, .. } => {
          let m = transform_at(*translate, *rotate, *pivot, f);
          for b in targets.bodies(d) {
            let total = m * pose.transform(b);
            pose.transforms.insert(b, total);
          }
        }
        ActionKind::Visibility { targets, show } => {
          for b in targets.bodies(d) {
            let was = pose.shows(b, shown(b));
            if was == *show {
              continue;
            }
            if f >= 1.0 {
              pose.visible.insert(b, *show);
            } else {
              // fading: drawn throughout, its opacity running between 0 and its own
              let base = pose.opacity.get(&b).copied().unwrap_or(1.0);
              pose.visible.insert(b, true);
              let fade = if *show { f } else { 1.0 - f };
              pose.opacity.insert(b, base * fade);
            }
          }
        }
        ActionKind::Opacity { targets, opacity } => {
          for b in targets.bodies(d) {
            let from = pose.opacity.get(&b).copied().unwrap_or(1.0);
            let o = from + (opacity.clamp(0.0, 1.0) - from) * f;
            if o >= 1.0 - 1e-9 {
              pose.opacity.remove(&b);
            } else {
              pose.opacity.insert(b, o);
            }
          }
        }
        // placed once every move up to `t` is in (they move with their body)
        ActionKind::Callout { .. } => callouts.push(a),
        ActionKind::View { from, to } => pose.camera = Some(from.lerp(to, f)),
      }
    }
    for a in callouts {
      if let ActionKind::Callout { anchor, body, text } = &a.kind {
        let at = body.map(|b| pose.transform(b)).unwrap_or(DAffine3::IDENTITY).transform_point3(DVec3::from(*anchor));
        pose.callouts.push(ShownCallout { action: a.id, anchor: at, text: text.clone() });
      }
    }
    // the transforms that came back to where they started are no move at all
    pose.transforms.retain(|_, m| !m.abs_diff_eq(DAffine3::IDENTITY, 1e-9));
    pose
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn cam(yaw: f64, distance: f64) -> AnimCamera {
    AnimCamera { target: [0.0; 3], yaw, pitch: 0.3, distance, fov: 0.5, ortho: false }
  }

  #[test]
  fn transforms_run_over_their_length_and_stack() {
    let mut d = Design::new("a");
    let body = BodyId(1);
    let mut an = Animation::default();
    let targets = Targets { components: vec![], bodies: vec![body] };
    let kind = |tr: [f64; 3]| ActionKind::Transform {
      targets: targets.clone(),
      translate: tr,
      rotate: [0.0; 3],
      pivot: [0.0; 3],
      source: TransformSource::Transform,
    };
    an.add_action(0, 0.0, 2.0, kind([10.0, 0.0, 0.0]));
    an.add_action(0, 2.0, 1.0, kind([0.0, 0.0, 5.0]));
    d.animation = an.clone();
    let shown = |_| true;
    let at = |t: f64| an.pose(&d, 0, t, &shown).transform(body).transform_point3(DVec3::ZERO);
    assert!(at(0.0).abs_diff_eq(DVec3::ZERO, 1e-9));
    assert!(at(1.0).abs_diff_eq(DVec3::new(5.0, 0.0, 0.0), 1e-9));
    assert!(at(2.5).abs_diff_eq(DVec3::new(10.0, 0.0, 2.5), 1e-9));
    assert!(at(9.0).abs_diff_eq(DVec3::new(10.0, 0.0, 5.0), 1e-9));
    assert_eq!(an.listed()[0].length(), 3.0);
  }

  #[test]
  fn a_turn_keeps_its_pivot_and_hiding_fades() {
    let d = Design::new("a");
    let body = BodyId(1);
    let mut an = Animation::default();
    let targets = Targets { components: vec![], bodies: vec![body] };
    an.add_action(
      0,
      0.0,
      1.0,
      ActionKind::Transform {
        targets: targets.clone(),
        translate: [0.0; 3],
        rotate: [0.0, 0.0, 90.0],
        pivot: [10.0, 0.0, 0.0],
        source: TransformSource::Transform,
      },
    );
    an.add_action(0, 1.0, 1.0, ActionKind::Visibility { targets, show: false });
    let p = an.pose(&d, 0, 1.0, &|_| true);
    assert!(
      p.transform(body).transform_point3(DVec3::new(10.0, 0.0, 0.0)).abs_diff_eq(DVec3::new(10.0, 0.0, 0.0), 1e-9)
    );
    assert!(
      p.transform(body).transform_point3(DVec3::new(11.0, 0.0, 0.0)).abs_diff_eq(DVec3::new(10.0, 1.0, 0.0), 1e-9)
    );
    let mid = an.pose(&d, 0, 1.5, &|_| true);
    assert_eq!(mid.visible.get(&body), Some(&true));
    assert!((mid.opacity[&body] - 0.5).abs() < 1e-9);
    let end = an.pose(&d, 0, 2.0, &|_| true);
    assert_eq!(end.visible.get(&body), Some(&false));
    assert!(!end.shows(body, true));
  }

  #[test]
  fn start_from_end_carries_on_and_views_glide() {
    let d = Design::new("a");
    let body = BodyId(1);
    let mut an = Animation::default();
    let targets = Targets { components: vec![], bodies: vec![body] };
    an.add_action(
      0,
      0.0,
      1.0,
      ActionKind::Transform {
        targets,
        translate: [0.0, 4.0, 0.0],
        rotate: [0.0; 3],
        pivot: [0.0; 3],
        source: TransformSource::Transform,
      },
    );
    let i = an.add_storyboard(None, StoryboardKind::StartFromEnd);
    assert_eq!(an.storyboards[i].name, "Storyboard2");
    assert_eq!(an.next_name(), "Storyboard3");
    let p = an.pose(&d, i, 0.0, &|_| true);
    assert!(p.transform(body).translation.abs_diff_eq(DVec3::new(0.0, 4.0, 0.0), 1e-9));
    an.add_action(i, 0.0, 2.0, ActionKind::View { from: cam(3.0, 100.0), to: cam(-3.0, 400.0) });
    let c = an.pose(&d, i, 1.0, &|_| true).camera.unwrap();
    assert!((c.distance - 200.0).abs() < 1e-6, "zooms evenly");
    // from 3 to -3 radians the short way is across ±π
    assert!(c.yaw.abs() > 3.0, "{}", c.yaw);
    assert!(an.pose(&d, 0, 0.5, &|_| true).camera.is_none());
  }
}
