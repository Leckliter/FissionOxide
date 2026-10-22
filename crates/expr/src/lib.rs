//! Parameter expressions, the way Fusion's dimension and parameter fields accept them.
//!
//! `"40 mm"`, `"width / 2 + 3"`, `"1.5 in"`, `"30 deg"`, `"sqrt(a^2 + b^2)"`.
//!
//! Comparisons (`<`, `<=`, `>`, `>=`, `==`, `!=`) give 1 or 0, and `if(cond, a, b)`, `and`, `or`
//! and `not` combine them, for configuration rules such as `if(length > 100 mm, 8 mm, 5 mm)`.
//!
//! Lengths evaluate to millimetres and angles to radians. A value written without any unit takes
//! the default unit of the quantity being asked for (millimetres for a length, degrees for an
//! angle), so `"45"` typed into an angle field means 45°.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unreachable)]

use std::collections::HashMap;
use std::f64::consts::PI;

/// Hostile input guard: expressions longer than this are rejected before parsing.
const MAX_EXPR_LEN: usize = 4096;
/// Parenthesis / unary nesting limit (keeps recursion bounded).
const MAX_DEPTH: usize = 64;

/// What kind of quantity an expression is expected to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
  #[default]
  Length,
  Angle,
  Unitless,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ExprError {
  #[error("empty expression")]
  Empty,
  #[error("expression too long")]
  TooLong,
  #[error("expression nested too deeply")]
  TooDeep,
  #[error("unexpected character '{0}'")]
  UnexpectedChar(char),
  #[error("unexpected end of expression")]
  UnexpectedEnd,
  #[error("unexpected '{0}'")]
  UnexpectedToken(String),
  #[error("unknown parameter or unit '{0}'")]
  Unknown(String),
  #[error("unknown function '{0}'")]
  UnknownFunction(String),
  #[error("'{0}' takes {1} argument(s)")]
  Arity(String, usize),
  #[error("result is not a finite number")]
  NotFinite,
  #[error("parameter '{0}' refers to itself")]
  Cycle(String),
}

/// An evaluated quantity: the number in base units (mm, rad) and whether any unit (or a
/// parameter, which carries its own unit) took part, so bare numbers can take the field's default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Value {
  pub value: f64,
  pub has_unit: bool,
}

/// Name → value lookup for parameters (values already in base units).
pub trait Scope {
  fn get(&self, name: &str) -> Option<f64>;
}

impl Scope for HashMap<String, f64> {
  fn get(&self, name: &str) -> Option<f64> {
    HashMap::get(self, name).copied()
  }
}

/// No parameters.
pub struct Empty;

impl Scope for Empty {
  fn get(&self, _name: &str) -> Option<f64> {
    None
  }
}

/// Evaluates `src` as a quantity of `kind`, returning base units (mm or radians).
pub fn eval(src: &str, kind: Kind, scope: &dyn Scope) -> Result<f64, ExprError> {
  let v = eval_raw(src, scope)?;
  let value = if v.has_unit {
    v.value
  } else {
    match kind {
      Kind::Angle => v.value * PI / 180.0,
      Kind::Length | Kind::Unitless => v.value,
    }
  };
  if value.is_finite() { Ok(value) } else { Err(ExprError::NotFinite) }
}

/// Evaluates without applying a default unit.
pub fn eval_raw(src: &str, scope: &dyn Scope) -> Result<Value, ExprError> {
  if src.len() > MAX_EXPR_LEN {
    return Err(ExprError::TooLong);
  }
  let tokens = lex(src)?;
  if tokens.is_empty() {
    return Err(ExprError::Empty);
  }
  let mut p = Parser { tokens, pos: 0, scope, depth: 0 };
  let v = p.expr()?;
  if let Some(t) = p.peek() {
    return Err(ExprError::UnexpectedToken(t.to_string()));
  }
  if v.value.is_finite() { Ok(v) } else { Err(ExprError::NotFinite) }
}

/// Identifiers an expression refers to (parameter names and units alike), for dependency order.
pub fn identifiers(src: &str) -> Vec<String> {
  match lex(src) {
    Ok(tokens) => tokens
      .into_iter()
      .filter_map(|t| match t {
        Token::Ident(s) => Some(s),
        _ => None,
      })
      .collect(),
    Err(_) => Vec::new(),
  }
}

/// Formats a length in mm the way the UI shows it.
pub fn format_length(mm: f64) -> String {
  format!("{} mm", trim_number(mm))
}

/// Formats an angle (radians) in degrees.
pub fn format_angle(rad: f64) -> String {
  format!("{} deg", trim_number(rad * 180.0 / PI))
}

/// Up to 4 decimals, without trailing zeros.
pub fn trim_number(v: f64) -> String {
  let s = format!("{v:.4}");
  let s = s.trim_end_matches('0').trim_end_matches('.');
  if s == "-0" { "0".to_string() } else { s.to_string() }
}

/// `src` re-spelt the way Fusion writes an expression back into a field once it is accepted:
/// one space between tokens ("d1*2" → "d1 * 2", "1/2" → "1 / 2"), none inside a function's
/// parentheses opening or before a comma, and a unary sign kept on its number ("-2 * 3").
/// `None` when `src` doesn't lex.
pub fn spaced(src: &str) -> Option<String> {
  let tokens = lex(src).ok()?;
  // a `-` or `+` not after a value or a `)` is a sign, not an operator
  let operand = |i: usize| matches!(tokens.get(i), Some(Token::Num(_) | Token::Ident(_) | Token::Op(')')));
  let sign = |i: usize| matches!(tokens.get(i), Some(Token::Op('-' | '+'))) && (i == 0 || !operand(i - 1));
  let mut out = String::new();
  // which open parentheses are a function call's (their contents hug them: "sqrt(a ^ 2)")
  let mut calls: Vec<bool> = Vec::new();
  for (i, t) in tokens.iter().enumerate() {
    let call_paren = matches!(t, Token::Op('('))
      && i > 0
      && matches!(&tokens[i - 1], Token::Ident(n) if FUNCTIONS.contains(&n.as_str()));
    let after_call_paren = i > 0 && matches!(tokens[i - 1], Token::Op('(')) && calls.last() == Some(&true);
    let closes_call = matches!(t, Token::Op(')')) && calls.last() == Some(&true);
    let glued = i == 0 || sign(i - 1) || call_paren || after_call_paren || closes_call || matches!(t, Token::Op(','));
    if !glued {
      out.push(' ');
    }
    match t {
      Token::Op('(') => calls.push(call_paren),
      Token::Op(')') => {
        calls.pop();
      }
      _ => {}
    }
    match t {
      Token::Num(n) => out.push_str(&format!("{n}")),
      Token::Ident(s) => out.push_str(s),
      Token::Op(c) => out.push(*c),
    }
  }
  Some(out)
}

/// Is `name` usable as a parameter name (not a unit or function, valid identifier)?
pub fn is_valid_parameter_name(name: &str) -> bool {
  let mut chars = name.chars();
  let first_ok = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_');
  first_ok
    && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    && unit_factor(name).is_none()
    && !FUNCTIONS.contains(&name)
    && name != "pi"
}

const FUNCTIONS: &[&str] = &[
  "sqrt", "abs", "sin", "cos", "tan", "asin", "acos", "atan", "min", "max", "round", "floor", "ceil", "if", "and",
  "or", "not",
];

/// Unit multiplier into base units (mm, rad).
fn unit_factor(name: &str) -> Option<f64> {
  Some(match name {
    "mm" => 1.0,
    "cm" => 10.0,
    "m" => 1000.0,
    "um" => 0.001,
    "in" => 25.4,
    "ft" => 304.8,
    "deg" => PI / 180.0,
    "rad" => 1.0,
    _ => return None,
  })
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
  Num(f64),
  Ident(String),
  Op(char),
}

impl std::fmt::Display for Token {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Token::Num(n) => write!(f, "{n}"),
      Token::Ident(s) => write!(f, "{s}"),
      Token::Op(c) => write!(f, "{c}"),
    }
  }
}

fn lex(src: &str) -> Result<Vec<Token>, ExprError> {
  let chars: Vec<char> = src.chars().collect();
  let mut out = Vec::new();
  let mut i = 0;
  while let Some(&c) = chars.get(i) {
    if c.is_whitespace() {
      i += 1;
    } else if c.is_ascii_digit() || c == '.' {
      let start = i;
      while chars.get(i).is_some_and(|c| c.is_ascii_digit() || *c == '.') {
        i += 1;
      }
      // exponent: 1e-3 (only when followed by a digit or sign+digit, so "2 e" isn't eaten)
      if chars.get(i).is_some_and(|c| *c == 'e' || *c == 'E') {
        let mut j = i + 1;
        if chars.get(j).is_some_and(|c| *c == '+' || *c == '-') {
          j += 1;
        }
        if chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
          i = j;
          while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
          }
        }
      }
      let text: String = chars.get(start..i).unwrap_or_default().iter().collect();
      let n: f64 = text.parse().map_err(|_| ExprError::UnexpectedToken(text.clone()))?;
      out.push(Token::Num(n));
    } else if c.is_alphabetic() || c == '_' {
      let start = i;
      while chars.get(i).is_some_and(|c| c.is_alphanumeric() || *c == '_') {
        i += 1;
      }
      out.push(Token::Ident(chars.get(start..i).unwrap_or_default().iter().collect()));
    } else if "+-*/^(),".contains(c) {
      out.push(Token::Op(c));
      i += 1;
    } else if "<>=!".contains(c) {
      // comparisons: `<=` `>=` `==` `!=` are one token each (`≤` `≥` `=` `≠`)
      let eq = chars.get(i + 1) == Some(&'=');
      let op = match (c, eq) {
        ('<', false) => '<',
        ('>', false) => '>',
        ('<', true) => '≤',
        ('>', true) => '≥',
        ('=', true) => '=',
        ('!', true) => '≠',
        _ => return Err(ExprError::UnexpectedChar(c)),
      };
      out.push(Token::Op(op));
      i += if eq { 2 } else { 1 };
    } else if c == '°' {
      out.push(Token::Ident("deg".into()));
      i += 1;
    } else if c == '"' {
      out.push(Token::Ident("in".into()));
      i += 1;
    } else {
      return Err(ExprError::UnexpectedChar(c));
    }
  }
  Ok(out)
}

struct Parser<'a> {
  tokens: Vec<Token>,
  pos: usize,
  scope: &'a dyn Scope,
  depth: usize,
}

impl Parser<'_> {
  fn peek(&self) -> Option<&Token> {
    self.tokens.get(self.pos)
  }

  fn next(&mut self) -> Option<Token> {
    let t = self.tokens.get(self.pos).cloned();
    self.pos += 1;
    t
  }

  fn eat(&mut self, op: char) -> bool {
    if self.peek() == Some(&Token::Op(op)) {
      self.pos += 1;
      true
    } else {
      false
    }
  }

  fn enter(&mut self) -> Result<(), ExprError> {
    self.depth += 1;
    if self.depth > MAX_DEPTH { Err(ExprError::TooDeep) } else { Ok(()) }
  }

  // expr := sum (cmp sum)?   (a comparison is 1 or 0, without a unit)
  fn expr(&mut self) -> Result<Value, ExprError> {
    let a = self.sum()?;
    let Some(Token::Op(op @ ('<' | '>' | '≤' | '≥' | '=' | '≠'))) = self.peek().cloned() else { return Ok(a) };
    self.pos += 1;
    let b = self.sum()?;
    // equal within a relative tolerance, so `0.1 + 0.2 == 0.3` and `1 in == 25.4 mm` hold
    let same = (a.value - b.value).abs() <= 1e-9 * a.value.abs().max(b.value.abs()).max(1.0);
    let t = match op {
      '<' => a.value < b.value && !same,
      '>' => a.value > b.value && !same,
      '≤' => a.value < b.value || same,
      '≥' => a.value > b.value || same,
      '=' => same,
      _ => !same,
    };
    Ok(Value { value: if t { 1.0 } else { 0.0 }, has_unit: false })
  }

  // sum := term (('+'|'-') term)*
  fn sum(&mut self) -> Result<Value, ExprError> {
    let mut acc = self.term()?;
    loop {
      if self.eat('+') {
        let r = self.term()?;
        acc = Value { value: acc.value + r.value, has_unit: acc.has_unit || r.has_unit };
      } else if self.eat('-') {
        let r = self.term()?;
        acc = Value { value: acc.value - r.value, has_unit: acc.has_unit || r.has_unit };
      } else {
        return Ok(acc);
      }
    }
  }

  // term := unary (('*'|'/') unary | implicit-unit)*
  fn term(&mut self) -> Result<Value, ExprError> {
    let mut acc = self.unary()?;
    loop {
      if self.eat('*') {
        let r = self.unary()?;
        acc = Value { value: acc.value * r.value, has_unit: acc.has_unit || r.has_unit };
      } else if self.eat('/') {
        let r = self.unary()?;
        // length / length is unitless; anything else keeps the unit flag of the numerator
        acc = Value { value: acc.value / r.value, has_unit: acc.has_unit && !r.has_unit };
      } else {
        return Ok(acc);
      }
    }
  }

  fn unary(&mut self) -> Result<Value, ExprError> {
    self.enter()?;
    let v = if self.eat('-') {
      let v = self.unary()?;
      Value { value: -v.value, has_unit: v.has_unit }
    } else if self.eat('+') {
      self.unary()?
    } else {
      self.power()?
    };
    self.depth -= 1;
    Ok(v)
  }

  // power := postfix ('^' unary)?
  fn power(&mut self) -> Result<Value, ExprError> {
    let base = self.postfix()?;
    if self.eat('^') {
      let e = self.unary()?;
      return Ok(Value { value: base.value.powf(e.value), has_unit: base.has_unit });
    }
    Ok(base)
  }

  // postfix := primary unit?   ("40 mm", "(a+b) in")
  fn postfix(&mut self) -> Result<Value, ExprError> {
    let v = self.primary()?;
    if let Some(Token::Ident(name)) = self.peek()
      && let Some(f) = unit_factor(name)
    {
      self.pos += 1;
      return Ok(Value { value: v.value * f, has_unit: true });
    }
    Ok(v)
  }

  fn primary(&mut self) -> Result<Value, ExprError> {
    match self.next() {
      None => Err(ExprError::UnexpectedEnd),
      Some(Token::Num(n)) => Ok(Value { value: n, has_unit: false }),
      Some(Token::Op('(')) => {
        self.enter()?;
        let v = self.expr()?;
        self.depth -= 1;
        if !self.eat(')') {
          return Err(ExprError::UnexpectedEnd);
        }
        Ok(v)
      }
      Some(Token::Ident(name)) => {
        if self.peek() == Some(&Token::Op('(')) {
          self.pos += 1;
          return self.call(&name);
        }
        if name == "pi" {
          return Ok(Value { value: PI, has_unit: false });
        }
        if let Some(v) = self.scope.get(&name) {
          return Ok(Value { value: v, has_unit: true });
        }
        Err(ExprError::Unknown(name))
      }
      Some(t) => Err(ExprError::UnexpectedToken(t.to_string())),
    }
  }

  fn call(&mut self, name: &str) -> Result<Value, ExprError> {
    self.enter()?;
    let mut args = Vec::new();
    if !self.eat(')') {
      loop {
        args.push(self.expr()?);
        if self.eat(')') {
          break;
        }
        if !self.eat(',') {
          return Err(ExprError::UnexpectedEnd);
        }
        if args.len() > 16 {
          return Err(ExprError::Arity(name.to_string(), 2));
        }
      }
    }
    self.depth -= 1;
    let one = |args: &[Value]| -> Result<Value, ExprError> {
      match args {
        [a] => Ok(*a),
        _ => Err(ExprError::Arity(name.to_string(), 1)),
      }
    };
    let two = |args: &[Value]| -> Result<(Value, Value), ExprError> {
      match args {
        [a, b] => Ok((*a, *b)),
        _ => Err(ExprError::Arity(name.to_string(), 2)),
      }
    };
    // Trig input: a bare number is degrees, an angle quantity is already radians.
    let angle_in = |v: Value| if v.has_unit { v.value } else { v.value * PI / 180.0 };
    let unitless = |value: f64| Value { value, has_unit: false };
    // Inverse trig returns an angle (radians, with unit).
    let angle_out = |value: f64| Value { value, has_unit: true };
    Ok(match name {
      "sqrt" => {
        let a = one(&args)?;
        Value { value: a.value.sqrt(), has_unit: a.has_unit }
      }
      "abs" => {
        let a = one(&args)?;
        Value { value: a.value.abs(), has_unit: a.has_unit }
      }
      "round" | "floor" | "ceil" => {
        let a = one(&args)?;
        let f = match name {
          "round" => a.value.round(),
          "floor" => a.value.floor(),
          _ => a.value.ceil(),
        };
        Value { value: f, has_unit: a.has_unit }
      }
      "sin" => unitless(angle_in(one(&args)?).sin()),
      "cos" => unitless(angle_in(one(&args)?).cos()),
      "tan" => unitless(angle_in(one(&args)?).tan()),
      "asin" => angle_out(one(&args)?.value.asin()),
      "acos" => angle_out(one(&args)?.value.acos()),
      "atan" => angle_out(one(&args)?.value.atan()),
      "min" | "max" => {
        let (a, b) = two(&args)?;
        let v = if name == "min" { a.value.min(b.value) } else { a.value.max(b.value) };
        Value { value: v, has_unit: a.has_unit || b.has_unit }
      }
      "if" => match args.as_slice() {
        [c, a, b] => {
          if c.value != 0.0 {
            *a
          } else {
            *b
          }
        }
        _ => return Err(ExprError::Arity(name.to_string(), 3)),
      },
      "and" | "or" => {
        let (a, b) = two(&args)?;
        let (a, b) = (a.value != 0.0, b.value != 0.0);
        unitless(if (name == "and" && a && b) || (name == "or" && (a || b)) { 1.0 } else { 0.0 })
      }
      "not" => unitless(if one(&args)?.value == 0.0 { 1.0 } else { 0.0 }),
      _ => return Err(ExprError::UnknownFunction(name.to_string())),
    })
  }
}

/// Evaluates a set of named parameters that may reference each other, in dependency order.
///
/// Returns each parameter's value (base units) or the error that stopped it. A cycle or a
/// reference to a failed parameter fails every parameter on it.
pub fn eval_parameters(params: &[(String, String, Kind)]) -> HashMap<String, Result<f64, ExprError>> {
  let mut done: HashMap<String, Result<f64, ExprError>> = HashMap::new();
  let by_name: HashMap<&str, (&str, Kind)> = params.iter().map(|(n, e, k)| (n.as_str(), (e.as_str(), *k))).collect();
  for (name, _, _) in params {
    let mut stack = Vec::new();
    resolve(name, &by_name, &mut done, &mut stack);
  }
  done
}

fn resolve(
  name: &str,
  by_name: &HashMap<&str, (&str, Kind)>,
  done: &mut HashMap<String, Result<f64, ExprError>>,
  stack: &mut Vec<String>,
) {
  if done.contains_key(name) {
    return;
  }
  let Some(&(src, kind)) = by_name.get(name) else { return };
  if stack.iter().any(|s| s == name) || stack.len() > MAX_DEPTH {
    done.insert(name.to_string(), Err(ExprError::Cycle(name.to_string())));
    return;
  }
  stack.push(name.to_string());
  for dep in identifiers(src) {
    if by_name.contains_key(dep.as_str()) {
      resolve(&dep, by_name, done, stack);
    }
  }
  stack.pop();
  let scope: HashMap<String, f64> = done.iter().filter_map(|(k, v)| v.as_ref().ok().map(|v| (k.clone(), *v))).collect();
  let r = eval(src, kind, &scope);
  // A reference to a parameter that failed reports the cycle/failure rather than "unknown".
  let r = match r {
    Err(ExprError::Unknown(n)) if matches!(done.get(&n), Some(Err(_))) => Err(ExprError::Cycle(n)),
    other => other,
  };
  done.entry(name.to_string()).or_insert(r);
}

#[cfg(test)]
mod tests {
  use super::*;

  fn len(s: &str) -> f64 {
    eval(s, Kind::Length, &Empty).unwrap()
  }

  #[test]
  fn spaced_rewrites_an_expression_as_fusion_does() {
    assert_eq!(spaced("d1*2").as_deref(), Some("d1 * 2"));
    assert_eq!(spaced("1/2").as_deref(), Some("1 / 2"));
    assert_eq!(spaced("( 1 / 2 ) * 1 in").as_deref(), Some("( 1 / 2 ) * 1 in"));
    assert_eq!(spaced("-2*3").as_deref(), Some("-2 * 3"));
    assert_eq!(spaced("2*-3").as_deref(), Some("2 * -3"));
    assert_eq!(spaced("sqrt(a^2+b^2)").as_deref(), Some("sqrt(a ^ 2 + b ^ 2)"));
    assert_eq!(spaced("max(1,2 mm)").as_deref(), Some("max(1, 2 mm)"));
    assert_eq!(spaced("0.5in").as_deref(), Some("0.5 in"));
    assert_eq!(spaced("2 $"), None);
  }

  #[test]
  fn numbers_and_units() {
    assert_eq!(len("40"), 40.0);
    assert_eq!(len("40 mm"), 40.0);
    assert_eq!(len("4cm"), 40.0);
    assert_eq!(len("1 in"), 25.4);
    assert_eq!(len("1\""), 25.4);
    assert_eq!(len("1.5e1"), 15.0);
    assert!((eval("90", Kind::Angle, &Empty).unwrap() - PI / 2.0).abs() < 1e-12);
    assert!((eval("90°", Kind::Angle, &Empty).unwrap() - PI / 2.0).abs() < 1e-12);
    assert!((eval("1 rad", Kind::Angle, &Empty).unwrap() - 1.0).abs() < 1e-12);
  }

  #[test]
  fn arithmetic_and_precedence() {
    assert_eq!(len("2 + 3 * 4"), 14.0);
    assert_eq!(len("(2 + 3) * 4"), 20.0);
    assert_eq!(len("-2^2"), -4.0);
    assert_eq!(len("2^3^2"), 512.0);
    assert_eq!(len("10 / 4"), 2.5);
    assert_eq!(len("sqrt(3^2 + 4^2)"), 5.0);
    assert_eq!(len("max(1, 2 cm)"), 20.0);
    assert!((len("sin(30)") - 0.5).abs() < 1e-12);
  }

  #[test]
  fn parameters() {
    let mut scope = HashMap::new();
    scope.insert("width".to_string(), 40.0);
    assert_eq!(eval("width / 2 + 3", Kind::Length, &scope).unwrap(), 23.0);
    assert_eq!(eval("width*2", Kind::Length, &scope).unwrap(), 80.0);
    assert!(matches!(eval("height", Kind::Length, &scope), Err(ExprError::Unknown(_))));
  }

  #[test]
  fn parameter_dependencies_and_cycles() {
    let params = vec![
      ("b".to_string(), "a * 2".to_string(), Kind::Length),
      ("a".to_string(), "10 mm".to_string(), Kind::Length),
      ("x".to_string(), "y + 1".to_string(), Kind::Length),
      ("y".to_string(), "x + 1".to_string(), Kind::Length),
      ("ang".to_string(), "45".to_string(), Kind::Angle),
    ];
    let r = eval_parameters(&params);
    assert_eq!(r["a"], Ok(10.0));
    assert_eq!(r["b"], Ok(20.0));
    assert!(r["x"].is_err());
    assert!(r["y"].is_err());
    assert!((r["ang"].clone().unwrap() - PI / 4.0).abs() < 1e-12);
  }

  #[test]
  fn hostile_input_does_not_panic() {
    for s in ["", "(", ")", "((((", "1 +", "*", "1e", "1e+", "..", "1..2", "€", "f(", "sqrt(1,2)", "1/0", "max()"] {
      let _ = eval(s, Kind::Length, &Empty);
    }
    let deep = "(".repeat(10_000) + "1" + &")".repeat(10_000);
    assert!(eval(&deep, Kind::Length, &Empty).is_err());
    let neg = "-".repeat(10_000) + "1";
    assert!(eval(&neg, Kind::Length, &Empty).is_err());
    assert_eq!(eval("1/0", Kind::Length, &Empty), Err(ExprError::NotFinite));
  }

  #[test]
  fn comparisons_and_conditions() {
    let scope: HashMap<String, f64> = [("length".to_string(), 120.0)].into_iter().collect();
    let u = |s: &str| eval(s, Kind::Unitless, &scope).unwrap();
    assert_eq!(u("length > 100 mm"), 1.0);
    assert_eq!(u("length <= 100"), 0.0);
    assert_eq!(u("length >= 120 mm"), 1.0);
    assert_eq!(u("length == 12 cm"), 1.0);
    assert_eq!(u("length != 12 cm"), 0.0);
    assert_eq!(u("0.1 + 0.2 == 0.3"), 1.0);
    assert_eq!(u("and(length > 100, not(length > 200))"), 1.0);
    assert_eq!(u("or(length < 10, 0)"), 0.0);
    assert_eq!(eval("if(length > 100 mm, 8 mm, 5 mm)", Kind::Length, &scope).unwrap(), 8.0);
    assert_eq!(eval("if(length < 100 mm, 8, 5) * 2", Kind::Length, &scope).unwrap(), 10.0);
    assert!(eval("if(1, 2)", Kind::Length, &scope).is_err());
    assert!(eval("length = 3", Kind::Length, &scope).is_err());
    assert!(eval("1 < 2 < 3", Kind::Length, &scope).is_err());
    assert_eq!(identifiers("if(a >= b, c, 1 mm)"), ["if", "a", "b", "c", "mm"]);
  }

  #[test]
  fn parameter_names() {
    assert!(is_valid_parameter_name("width"));
    assert!(is_valid_parameter_name("d_1"));
    assert!(!is_valid_parameter_name("mm"));
    assert!(!is_valid_parameter_name("1abc"));
    assert!(!is_valid_parameter_name("sqrt"));
    assert!(!is_valid_parameter_name("a b"));
  }

  #[test]
  fn formatting() {
    assert_eq!(format_length(12.5), "12.5 mm");
    assert_eq!(format_length(10.0), "10 mm");
    assert_eq!(format_angle(PI / 2.0), "90 deg");
    assert_eq!(trim_number(-0.00001), "0");
  }
}
