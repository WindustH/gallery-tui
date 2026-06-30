use super::*;

#[test]
fn screen_rect_keeps_partially_visible_top_card() {
  let viewport = Rect::new(0, 0, 80, 10);
  let card = CanvasRect {
    x: 2,
    y: 0,
    width: 20,
    height: 16,
  };

  assert_eq!(screen_rect(card, viewport, 8), Some(Rect::new(2, 0, 20, 8)));
}

#[test]
fn screen_rect_drops_fully_hidden_card() {
  let viewport = Rect::new(0, 0, 80, 10);
  let card = CanvasRect {
    x: 2,
    y: 0,
    width: 20,
    height: 16,
  };

  assert_eq!(screen_rect(card, viewport, 16), None);
}
