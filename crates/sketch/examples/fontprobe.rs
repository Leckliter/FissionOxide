struct P(Vec<(f32, f32)>);
impl ttf_parser::OutlineBuilder for P {
  fn move_to(&mut self, x: f32, y: f32) { self.0.push((x, y)); }
  fn line_to(&mut self, x: f32, y: f32) { self.0.push((x, y)); }
  fn quad_to(&mut self, _: f32, _: f32, x: f32, y: f32) { self.0.push((x, y)); }
  fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, x: f32, y: f32) { self.0.push((x, y)); }
  fn close(&mut self) {}
}
fn main() {
  let data = std::fs::read("C:/Windows/Fonts/arial.ttf").unwrap();
  let face = ttf_parser::Face::parse(&data, 0).unwrap();
  for ch in "LIO".chars() {
    let g = face.glyph_index(ch).unwrap();
    let mut p = P(vec![]);
    face.outline_glyph(g, &mut p);
    println!("{ch} adv {:?} lsb {:?} {:?}", face.glyph_hor_advance(g), face.glyph_hor_side_bearing(g), if ch == 'L' { p.0.clone() } else { vec![] });
  }
}
