use super::*;

#[test]
fn parses_yazi_style_key_names() {
  assert_eq!(parse_key("<Enter>").as_deref(), Some("enter"));
  assert_eq!(parse_key("<PageDown>").as_deref(), Some("pgdn"));
  assert_eq!(parse_key("<C-c>").as_deref(), Some("ctrl-c"));
  assert_eq!(parse_key("q").as_deref(), Some("q"));
}

#[test]
fn contexts_are_separate() {
  let bindings = KeyBindings::from_config(&KeymapConfig::default());
  assert!(matches!(
    bindings.match_sequence(KeyContext::Browser, &[String::from("q")]),
    MatchResult::Action(action) if action == "quit"
  ));
  assert!(matches!(
    bindings.match_sequence(KeyContext::Detail, &[String::from("q")]),
    MatchResult::Action(action) if action == "back"
  ));
  assert!(matches!(
    bindings.match_sequence(KeyContext::Input, &[String::from("ctrl-g")]),
    MatchResult::Action(action) if action == "edit_in_editor"
  ));
  assert!(matches!(
    bindings.match_sequence(KeyContext::Input, &[String::from("q")]),
    MatchResult::None
  ));
}
