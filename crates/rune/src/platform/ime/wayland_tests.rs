use super::*;

/// A session Rune reset: enabled, disabled, enabled again.
fn after_a_reset() -> Session {
    let mut session = Session::default();
    session.enabling();
    session.disabling();
    session.enabling();
    session
}

#[test]
fn an_answer_for_the_session_a_reset_ended_is_late() {
    let session = after_a_reset();
    assert!(session.late(2), "sent before the compositor saw the new enable");
    assert!(!session.late(3) && !session.late(4));
}

#[test]
fn the_new_sessions_direct_commit_arrives_without_a_preedit_or_a_key() {
    let mut session = after_a_reset();
    session.commit_string(Some("，".into()));
    assert_eq!(session.done(3), [Ime::Preedit(String::new(), None), Ime::Commit("，".into())]);
}

#[test]
fn a_composition_in_the_new_session_shows_and_commits() {
    let mut session = after_a_reset();
    session.preedit(Some("ね".into()), 3, 3);
    assert_eq!(session.done(3), [Ime::Preedit("ね".into(), Some((3, 3)))]);
    session.commit_string(Some("猫".into()));
    assert_eq!(session.done(4), [Ime::Preedit(String::new(), None), Ime::Commit("猫".into())]);
}

#[test]
fn what_arrives_while_the_input_method_is_off_is_late() {
    let mut session = after_a_reset();
    session.disabling();
    assert!(session.late(4));
}

#[test]
fn a_serial_past_the_wrap_still_belongs_to_the_session() {
    let mut session = Session::default();
    session.commits = u32::MAX - 1;
    session.enabling();
    assert_eq!(session.since, u32::MAX);
    assert!(!session.late(u32::MAX) && !session.late(1));
    assert!(session.late(u32::MAX - 1));
}

#[test]
fn review_batch_keeps_both_preedit_and_commit_independent_of_event_order() {
    let mut s = after_a_reset();
    s.preedit(Some("next".into()), 4, 4);
    s.commit_string(Some("typed".into()));
    assert_eq!(s.done(3), [Ime::Preedit(String::new(), None), Ime::Commit("typed".into()), Ime::Preedit("next".into(), Some((4,4)))]);
}
