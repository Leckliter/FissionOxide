//! Bicubic B-spline patches: interpolation of a grid of points, and evaluation.

use glam::DVec3;

/// A non-rational bicubic B-spline surface: `poles[i][j]` (u index i, v index j) and, per
/// direction, the distinct knot values with their multiplicities (clamped ends).
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
  pub poles: Vec<Vec<DVec3>>,
  pub u_knots: Vec<f64>,
  pub u_mults: Vec<u32>,
  pub v_knots: Vec<f64>,
  pub v_mults: Vec<u32>,
}

impl Patch {
  pub const DEGREE: u32 = 3;

  /// The point at (u, v), both in 0..=1.
  pub fn eval(&self, u: f64, v: f64) -> DVec3 {
    let (ku, kv) = (flat(&self.u_knots, &self.u_mults), flat(&self.v_knots, &self.v_mults));
    let (su, bu) = span_basis(&ku, self.poles.len(), u);
    let (sv, bv) = span_basis(&kv, self.poles[0].len(), v);
    let mut p = DVec3::ZERO;
    for (a, wu) in bu.iter().enumerate() {
      for (b, wv) in bv.iter().enumerate() {
        p += *wu * *wv * self.poles[su - 3 + a][sv - 3 + b];
      }
    }
    p
  }

  /// The unit normal at (u, v) (from central differences, one-sided at the ends).
  pub fn normal(&self, u: f64, v: f64) -> DVec3 {
    let h = 1e-5;
    let (u0, u1) = ((u - h).max(0.0), (u + h).min(1.0));
    let (v0, v1) = ((v - h).max(0.0), (v + h).min(1.0));
    let du = self.eval(u1, v) - self.eval(u0, v);
    let dv = self.eval(u, v1) - self.eval(u, v0);
    du.cross(dv).normalize_or_zero()
  }
}

fn flat(knots: &[f64], mults: &[u32]) -> Vec<f64> {
  knots.iter().zip(mults).flat_map(|(k, m)| std::iter::repeat_n(*k, *m as usize)).collect()
}

/// The knot span holding `u` and the four basis functions non-zero there.
fn span_basis(knots: &[f64], n: usize, u: f64) -> (usize, [f64; 4]) {
  let u = u.clamp(0.0, 1.0);
  let span = if u >= knots[n] { n - 1 } else { (3..n).rev().find(|&s| knots[s] <= u).unwrap_or(3) };
  (span, basis(knots, span, u))
}

/// The bicubic B-spline through a grid of points (`grid[i][j]`, u index i), at uniform parameters
/// with not-a-knot ends.
pub fn interpolate(grid: &[Vec<DVec3>]) -> Result<Patch, String> {
  let (nu, nv) = (grid.len(), grid.first().map_or(0, Vec::len));
  if nu < 4 || nv < 4 {
    return Err("too few limit points to fit a surface".into());
  }
  let (ku, su) = Collocation::new(nu)?;
  let (kv, sv) = Collocation::new(nv)?;
  // along v for each row, then along u for each column
  let rows: Vec<Vec<DVec3>> = grid.iter().map(|row| sv.solve(row)).collect();
  let mut poles = vec![vec![DVec3::ZERO; nv]; nu];
  for j in 0..nv {
    let col: Vec<DVec3> = rows.iter().map(|r| r[j]).collect();
    for (i, p) in su.solve(&col).into_iter().enumerate() {
      poles[i][j] = p;
    }
  }
  Ok(Patch { poles, u_knots: ku.0, u_mults: ku.1, v_knots: kv.0, v_mults: kv.1 })
}

/// An LU-factored cubic interpolation matrix for `n` points at uniform parameters.
struct Collocation {
  lu: Vec<Vec<f64>>,
  perm: Vec<usize>,
}

impl Collocation {
  /// The solver and the distinct knots with multiplicities.
  fn new(n: usize) -> Result<((Vec<f64>, Vec<u32>), Collocation), String> {
    let m = (n - 1) as f64;
    let params: Vec<f64> = (0..n).map(|i| i as f64 / m).collect();
    // full knot vector: clamped ends, interior knots at the parameters but the second and last-but-one
    let mut knots = vec![0.0; 4];
    knots.extend(params.iter().take(n - 2).skip(2));
    knots.extend([1.0; 4]);
    let mut a = vec![vec![0.0; n]; n];
    for (i, &u) in params.iter().enumerate() {
      let span = if i == n - 1 { n - 1 } else { (3..n).rev().find(|&s| knots[s] <= u).unwrap_or(3) };
      for (r, b) in basis(&knots, span, u).into_iter().enumerate() {
        a[i][span - 3 + r] = b;
      }
    }
    let mut perm: Vec<usize> = (0..n).collect();
    for c in 0..n {
      let p = (c..n).max_by(|x, y| a[*x][c].abs().total_cmp(&a[*y][c].abs())).unwrap_or(c);
      if a[p][c].abs() < 1e-14 {
        return Err("singular interpolation".into());
      }
      a.swap(c, p);
      perm.swap(c, p);
      let pivot = a[c].clone();
      for row in a.iter_mut().skip(c + 1) {
        let f = row[c] / pivot[c];
        row[c] = f;
        for (x, y) in row.iter_mut().zip(&pivot).skip(c + 1) {
          *x -= f * y;
        }
      }
    }
    let mut distinct = vec![0.0];
    distinct.extend(params.iter().take(n - 2).skip(2));
    distinct.push(1.0);
    let mut mults = vec![1; distinct.len()];
    mults[0] = 4;
    if let Some(last) = mults.last_mut() {
      *last = 4;
    }
    Ok(((distinct, mults), Collocation { lu: a, perm }))
  }

  fn solve(&self, b: &[DVec3]) -> Vec<DVec3> {
    let n = b.len();
    let mut x: Vec<DVec3> = self.perm.iter().map(|&p| b[p]).collect();
    for r in 0..n {
      for c in 0..r {
        x[r] = x[r] - self.lu[r][c] * x[c];
      }
    }
    for r in (0..n).rev() {
      for c in r + 1..n {
        x[r] = x[r] - self.lu[r][c] * x[c];
      }
      x[r] /= self.lu[r][r];
    }
    x
  }
}

/// The four cubic basis functions that are non-zero on knot span `span` at `u`.
fn basis(knots: &[f64], span: usize, u: f64) -> [f64; 4] {
  let mut n = [1.0, 0.0, 0.0, 0.0];
  let (mut left, mut right) = ([0.0; 4], [0.0; 4]);
  for j in 1..=3 {
    left[j] = u - knots[span + 1 - j];
    right[j] = knots[span + j] - u;
    let mut saved = 0.0;
    for r in 0..j {
      let temp = n[r] / (right[r + 1] + left[j - r]);
      n[r] = saved + right[r + 1] * temp;
      saved = left[j - r] * temp;
    }
    n[j] = saved;
  }
  n
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn interpolation_passes_through_the_points() {
    let f = |u: f64, v: f64| DVec3::new(u, v, (3.0 * u).sin() * v * v);
    let grid: Vec<Vec<DVec3>> = (0..9).map(|i| (0..7).map(|j| f(i as f64 / 8.0, j as f64 / 6.0)).collect()).collect();
    let s = interpolate(&grid).unwrap();
    assert_eq!(s.poles.len(), 9);
    assert_eq!(s.u_mults.iter().sum::<u32>() as usize, 9 + 4);
    for (i, row) in grid.iter().enumerate() {
      for (j, p) in row.iter().enumerate() {
        assert!(s.eval(i as f64 / 8.0, j as f64 / 6.0).distance(*p) < 1e-9);
      }
    }
    // cubics are reproduced exactly between the points
    let g = |u: f64, v: f64| DVec3::new(u, v, u * u * u - v * v);
    let grid: Vec<Vec<DVec3>> = (0..9).map(|i| (0..7).map(|j| g(i as f64 / 8.0, j as f64 / 6.0)).collect()).collect();
    let s = interpolate(&grid).unwrap();
    assert!(s.eval(0.37, 0.71).distance(g(0.37, 0.71)) < 1e-9);
  }
}
