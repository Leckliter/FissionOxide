//! Release versions: `1.2.3`, with an optional `v` in front and a pre-release after a `-`.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
  /// Major, minor, patch (missing parts are 0).
  pub parts: [u64; 3],
  /// `beta.1` in `1.2.0-beta.1`; a pre-release sorts before its release.
  pub pre: Option<String>,
}

impl Version {
  /// Parses `1.2.3`, `v1.2`, `1.2.3-rc.1` or `1.2.3+build` (build metadata is ignored).
  pub fn parse(s: &str) -> Option<Version> {
    let s = s.trim();
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let s = s.split('+').next()?;
    let (core, pre) = match s.split_once('-') {
      Some((c, p)) if !p.is_empty() => (c, Some(p.to_string())),
      Some(_) => return None,
      None => (s, None),
    };
    let mut parts = [0u64; 3];
    for (n, p) in core.split('.').enumerate() {
      *parts.get_mut(n)? = p.parse().ok()?;
    }
    Some(Version { parts, pre })
  }
}

impl Ord for Version {
  fn cmp(&self, other: &Self) -> Ordering {
    self.parts.cmp(&other.parts).then_with(|| match (&self.pre, &other.pre) {
      (None, None) => Ordering::Equal,
      (None, Some(_)) => Ordering::Greater,
      (Some(_), None) => Ordering::Less,
      (Some(a), Some(b)) => cmp_pre(a, b),
    })
  }
}

impl PartialOrd for Version {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl std::fmt::Display for Version {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let [a, b, c] = self.parts;
    write!(f, "{a}.{b}.{c}")?;
    if let Some(p) = &self.pre {
      write!(f, "-{p}")?;
    }
    Ok(())
  }
}

/// Semver pre-release order: dot-separated identifiers, numbers numerically and before words.
fn cmp_pre(a: &str, b: &str) -> Ordering {
  let mut x = a.split('.');
  let mut y = b.split('.');
  loop {
    match (x.next(), y.next()) {
      (None, None) => return Ordering::Equal,
      (None, Some(_)) => return Ordering::Less,
      (Some(_), None) => return Ordering::Greater,
      (Some(p), Some(q)) => {
        let o = match (p.parse::<u64>(), q.parse::<u64>()) {
          (Ok(m), Ok(n)) => m.cmp(&n),
          (Ok(_), Err(_)) => Ordering::Less,
          (Err(_), Ok(_)) => Ordering::Greater,
          (Err(_), Err(_)) => p.cmp(q),
        };
        if o != Ordering::Equal {
          return o;
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
  }

  #[test]
  fn parses_and_orders() {
    assert_eq!(v("v1.2.3").parts, [1, 2, 3]);
    assert_eq!(v("1.2").parts, [1, 2, 0]);
    assert_eq!(v("0.1.0+abc").to_string(), "0.1.0");
    assert!(v("0.2.0") > v("0.1.9"));
    assert!(v("0.10.0") > v("0.9.0"));
    assert!(v("1.0.0") > v("1.0.0-rc.2"));
    assert!(v("1.0.0-rc.10") > v("1.0.0-rc.2"));
    assert!(v("1.0.0-beta") > v("1.0.0-alpha.5"));
    assert!(v("1.0.0-alpha.1") > v("1.0.0-alpha"));
    assert_eq!(v("v0.3.0"), v("0.3"));
    for bad in ["", "x", "1.2.3.4", "1.-", "1.a"] {
      assert!(Version::parse(bad).is_none(), "{bad}");
    }
  }
}
