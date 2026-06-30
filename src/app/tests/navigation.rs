use super::*;

#[test]
fn q_quits_in_browser() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();

  app.handle_input(key('q'), &tx);

  assert!(app.should_quit());
  assert_eq!(app.view, ViewMode::Browser);
}

#[test]
fn q_returns_from_detail_without_quitting() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.view = ViewMode::Detail;

  app.handle_input(key('q'), &tx);

  assert!(!app.should_quit());
  assert_eq!(app.view, ViewMode::Browser);
}

#[test]
fn esc_clears_selection_in_browser() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.images = vec![image("a.png"), image("b.png")];
  app.selected.insert(app.images[0].path.clone());
  app.selected.insert(app.images[1].path.clone());

  app.handle_input(
    Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
    &tx,
  );

  assert!(app.selected.is_empty());
  assert!(!app.should_quit());
  assert_eq!(app.message, "cleared 2 selected image(s)");
}

#[test]
fn mouse_click_focuses_browser_card() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.images = vec![image("a.png"), image("b.png")];
  app.update_browser_layout(
    BrowserLayout {
      cards: vec![
        CanvasRect {
          x: 0,
          y: 0,
          width: 10,
          height: 6,
        },
        CanvasRect {
          x: 12,
          y: 0,
          width: 10,
          height: 6,
        },
      ],
      total_height: 6,
      columns: 2,
    },
    Rect::new(5, 3, 40, 10),
  );

  app.handle_input(click(18, 4), &tx);

  assert_eq!(app.focused, 1);
}

#[test]
fn mouse_click_uses_browser_scroll_offset() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.images = vec![image("a.png"), image("b.png")];
  app.update_browser_layout(
    BrowserLayout {
      cards: vec![
        CanvasRect {
          x: 0,
          y: 0,
          width: 10,
          height: 6,
        },
        CanvasRect {
          x: 0,
          y: 8,
          width: 10,
          height: 6,
        },
      ],
      total_height: 14,
      columns: 1,
    },
    Rect::new(0, 0, 20, 6),
  );
  app.browser_scroll = 8;

  app.handle_input(click(4, 2), &tx);

  assert_eq!(app.focused, 1);
}

#[test]
fn mouse_wheel_navigates_between_browser_rows() {
  let (tx, _rx) = mpsc::unbounded_channel();
  let mut app = test_app();
  app.images = vec![
    image("a.png"),
    image("b.png"),
    image("c.png"),
    image("d.png"),
  ];
  app.focused = 1;
  app.update_browser_layout(
    BrowserLayout {
      cards: vec![
        CanvasRect {
          x: 0,
          y: 0,
          width: 10,
          height: 6,
        },
        CanvasRect {
          x: 12,
          y: 0,
          width: 10,
          height: 6,
        },
        CanvasRect {
          x: 0,
          y: 8,
          width: 10,
          height: 6,
        },
        CanvasRect {
          x: 12,
          y: 8,
          width: 10,
          height: 6,
        },
      ],
      total_height: 14,
      columns: 2,
    },
    Rect::new(0, 0, 30, 6),
  );

  app.handle_input(wheel(MouseEventKind::ScrollDown), &tx);
  assert_eq!(app.focused, 3);

  app.handle_input(wheel(MouseEventKind::ScrollUp), &tx);
  assert_eq!(app.focused, 1);
}

#[test]
fn tall_focused_card_does_not_oscillate_scroll() {
  let mut app = test_app();
  app.images = vec![image("a.png")];
  app.focused = 0;
  app.update_browser_layout(
    BrowserLayout {
      cards: vec![CanvasRect {
        x: 0,
        y: 0,
        width: 10,
        height: 40,
      }],
      total_height: 40,
      columns: 1,
    },
    Rect::new(0, 0, 20, 10),
  );
  app.browser_scroll = 30;

  app.ensure_focus_visible();
  assert_eq!(app.browser_scroll, 30);

  app.ensure_focus_visible();
  assert_eq!(app.browser_scroll, 30);
}
