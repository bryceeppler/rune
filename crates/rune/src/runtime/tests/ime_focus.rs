//! Focus leaving a field mid-composition, by every path and whatever the platform answers: the
//! text the field shows commits there before focus moves, which ends its input method session.
//! What the platform sends for that session afterwards, stamped with it, reaches no field;
//! anything from the current session always arrives.
use super::*;
use crate::{Ime, Key, KeyEvent, Mods, TextEdit, TextStyle, text_input};

struct Notes {
    a: TextEdit,
    b: TextEdit,
    show_a: bool,
    changed: Vec<&'static str>,
    /// Whether `a` still had focus each time it heard `on_input_change`.
    a_focused_at_change: Vec<bool>,
}

fn view(s: &Notes, _: &mut Cx<Notes>) -> El {
    let field = |edit: &TextEdit, key: &'static str| text_input(edit, TextStyle::new(14.0, 400.0)).key(key).size(200.0, 40.0);
    col().size(300.0, 200.0)
        .children(s.show_a.then(|| field(&s.a, "a")
            .on_edit(|s: &mut Notes| &mut s.a)
            .on_input_change(|s: &mut Notes, _| { s.changed.push("a"); s.a_focused_at_change.push(s.a.focused); })))
        .child(field(&s.b, "b")
            .on_edit(|s: &mut Notes| &mut s.b)
            .on_input_change(|s: &mut Notes, _| s.changed.push("b")))
        .child(el().key("button").size(100.0, 40.0).focusable(true).role("button"))
}

/// `a` composing `か` in the first session.
fn composing_in_a() -> Runtime<Notes> {
    let mut a = TextEdit::new("");
    a.focused = true;
    let notes = Notes { a, b: TextEdit::new(""), show_a: true, changed: Vec::new(), a_focused_at_change: Vec::new() };
    let mut rt = Runtime::new(notes, view, (300.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.ime(Ime::Preedit("か".into(), Some((0, 3))));
    rt.frame(0.0);
    assert!(rt.state.a.preedit.is_some(), "a composes");
    rt
}

/// The session `composing_in_a` composed in, which focus leaving ends.
const FIRST: u64 = 0;

/// Events from the current session.
fn send(rt: &mut Runtime<Notes>, events: impl IntoIterator<Item = Ime>) {
    for ev in events { rt.ime(ev); }
    rt.frame(0.0);
}

/// Events from `session`, as a shell that knows they answer it delivers them.
fn late(rt: &mut Runtime<Notes>, session: u64, events: impl IntoIterator<Item = Ime>) {
    for ev in events { rt.ime_in(session, ev); }
    rt.frame(0.0);
}

/// Events as the shell hears them from the platform.
fn platform(rt: &mut Runtime<Notes>, events: impl IntoIterator<Item = Ime>) {
    for ev in events { rt.ime_from_platform(ev, false); }
    rt.frame(0.0);
}

fn click(rt: &mut Runtime<Notes>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn press(rt: &mut Runtime<Notes>, key: Key, shift: bool) {
    rt.key(KeyEvent { key, mods: Mods { shift, ..Mods::default() } });
    rt.frame(0.0);
}

fn preedits_clear(rt: &Runtime<Notes>) -> bool {
    rt.state.a.preedit.is_none() && rt.state.b.preedit.is_none()
}

fn texts(rt: &Runtime<Notes>) -> (&str, &str) {
    (rt.state.a.text.as_str(), rt.state.b.text.as_str())
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Path { Field, Button, Blank, Request, Tab, BackTab, Deactivate, Removed, SecondWindow }

/// Moves focus off `a` by `path`; the window deactivates (or hides) as the shell reports it.
fn leave(rt: &mut Runtime<Notes>, path: Path) {
    match path {
        Path::Field => click(rt, 100.0, 60.0),
        Path::Button => click(rt, 50.0, 100.0),
        Path::Blank => click(rt, 280.0, 180.0),
        Path::Request => { rt.update(|s, _| s.b.focused = true); rt.frame(0.0); }
        Path::Tab | Path::BackTab => press(rt, Key::Tab, path == Path::BackTab),
        Path::Deactivate => {
            rt.window_focus(false);
            rt.cancel_composition();
            rt.frame(0.0);
            rt.window_focus(true);
            rt.update(|s, _| s.b.focused = true);
            rt.frame(0.0);
        }
        Path::Removed => { rt.update(|s, _| { s.show_a = false; s.b.focused = true; }); rt.frame(0.0); }
        Path::SecondWindow => { rt.window_focus(false); rt.cancel_composition(); rt.frame(0.0); }
    }
    assert!(rt.ime_reset, "{path:?}: the platform's input method resets");
    assert!(rt.ime_session() > FIRST, "{path:?}: the composition's session ended");
}

/// What the platform sends once its composition is discarded, all of it from the ended session.
#[derive(Clone, Copy)]
enum Answer {
    /// The input method commits its own conversion anyway.
    Commit,
    EmptyCommit,
    Cancel,
    /// The input method turns off and on before its commit arrives, as a compositor may.
    LateCommit,
}

fn answer(rt: &mut Runtime<Notes>, answer: Answer) {
    let clear = || Ime::Preedit(String::new(), None);
    let ended = rt.ime_session() - 1;
    match answer {
        Answer::Commit => late(rt, ended, [clear(), Ime::Commit("家".into()), Ime::Disabled]),
        Answer::EmptyCommit => late(rt, ended, [clear(), Ime::Commit(String::new()), Ime::Disabled]),
        Answer::Cancel => late(rt, ended, [clear(), Ime::Disabled]),
        Answer::LateCommit => {
            late(rt, ended, [Ime::Disabled, Ime::Enabled]);
            for _ in 0..3 { rt.frame(0.016); }
            late(rt, ended, [clear(), Ime::Commit("家".into()), Ime::Disabled]);
        }
    }
}

fn verify(path: Path, platform: Answer) {
    let mut rt = composing_in_a();
    let mut other = composing_in_a();
    send(&mut other, [Ime::Preedit(String::new(), None), Ime::Commit(String::new())]);
    leave(&mut rt, path);
    if path == Path::Removed {
        assert_eq!(rt.state.a.text, "", "no view reaches a removed field's model");
    } else {
        assert_eq!(rt.state.a.text, "か", "{path:?}: what a showed commits there before focus moves");
        assert_eq!(rt.state.changed, ["a"], "{path:?}: a hears on_input_change");
    }
    answer(&mut rt, platform);
    if path == Path::Removed { rt.update(|s, _| s.show_a = true); rt.frame(0.0); }
    assert_eq!(rt.state.a.text, "か", "{path:?}: a keeps what it showed, not the platform's late answer");
    assert_eq!(rt.state.b.text, "", "{path:?}: the field that took focus gets nothing of it");
    assert_eq!(rt.state.changed, ["a"], "{path:?}: a hears on_input_change once");
    assert!(preedits_clear(&rt), "{path:?}: no preedit stays");
    assert!(other.state.a.text.is_empty() && other.state.changed.is_empty(), "another window's runtime is untouched");
    click(&mut rt, 100.0, 60.0);
    send(&mut rt, [Ime::Enabled, Ime::Preedit("ね".into(), None), Ime::Preedit(String::new(), None), Ime::Commit("猫".into())]);
    assert_eq!(rt.state.b.text, "猫", "{path:?}: b's own composition is b's");
    assert_eq!(rt.state.changed, ["a", "b"]);
}

macro_rules! paths {
    ($($name:ident: $path:ident),* $(,)?) => { $(mod $name {
        use super::*;
        #[test] fn commit() { verify(Path::$path, Answer::Commit); }
        #[test] fn empty_commit() { verify(Path::$path, Answer::EmptyCommit); }
        #[test] fn cancel() { verify(Path::$path, Answer::Cancel); }
        #[test] fn asynchronous_commit_after_disable_enable() { verify(Path::$path, Answer::LateCommit); }
    })* };
}
paths!(
    pointer_field: Field, pointer_button: Button, pointer_blank: Blank, programmatic: Request, tab: Tab,
    shift_tab: BackTab, deactivate_reactivate: Deactivate, removed: Removed, second_window: SecondWindow,
);

#[test]
fn the_composing_field_hears_on_input_change_before_focus_moves() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    assert_eq!(rt.state.changed, ["a"], "a's change runs as the click lands, before any platform answer");
    assert_eq!(rt.state.a_focused_at_change, [true], "a still had focus when it heard it");
    assert!(rt.state.b.focused && !rt.state.a.focused);
}

#[test]
fn a_commit_before_focus_moves_keeps_the_input_methods_conversion() {
    let mut rt = composing_in_a();
    send(&mut rt, [Ime::Preedit(String::new(), None), Ime::Commit("家".into())]);
    click(&mut rt, 100.0, 60.0);
    assert_eq!(rt.state.a.text, "家");
    assert_eq!(rt.state.changed, ["a"]);
    assert_eq!(rt.ime_session(), FIRST, "no composition was open to end its session");
}

#[test]
fn an_empty_preedit_commits_nothing_and_the_next_commit_is_the_new_fields() {
    let mut rt = composing_in_a();
    send(&mut rt, [Ime::Preedit(String::new(), None)]);
    click(&mut rt, 100.0, 60.0);
    send(&mut rt, [Ime::Preedit(String::new(), None), Ime::Commit("，".into())]);
    assert_eq!(texts(&rt), ("", "，"));
    assert_eq!(rt.state.changed, ["b"]);
}

#[test]
fn the_ended_sessions_events_drop_and_the_current_ones_arrive_interleaved() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    late(&mut rt, FIRST, [Ime::Preedit(String::new(), None)]);
    press(&mut rt, Key::Text("x".into()), false);
    send(&mut rt, [Ime::Preedit(String::new(), None), Ime::Commit("，".into())]);
    late(&mut rt, FIRST, [Ime::Preedit("かな".into(), None), Ime::Commit("家".into())]);
    send(&mut rt, [Ime::Commit("。".into())]);
    assert_eq!(texts(&rt), ("か", "x，。"));
}

#[test]
fn a_new_model_under_the_same_field_never_hears_the_old_composition() {
    let mut rt = composing_in_a();
    rt.update(|s, _| { s.a = TextEdit::new("next note"); s.a.focused = true; });
    rt.frame(0.0);
    assert!(rt.ime_reset, "the platform input method resets for the new note");
    answer(&mut rt, Answer::Commit);
    assert_eq!(rt.state.a.text, "next note", "the old note's composition stays out of the new note");
    assert!(rt.state.changed.is_empty() && preedits_clear(&rt));
}

#[test]
fn a_late_preedit_from_the_ended_session_stays_out_of_the_new_field() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    late(&mut rt, FIRST, [Ime::Disabled, Ime::Enabled, Ime::Preedit("かな".into(), None)]);
    assert!(rt.state.b.preedit.is_none(), "the old session's preedit reached b: {:?}", rt.state.b.preedit);
    late(&mut rt, FIRST, [Ime::Preedit(String::new(), None), Ime::Commit("仮名".into())]);
    assert_eq!(texts(&rt), ("か", ""));
}

#[test]
fn a_late_preedit_and_conversion_type_nothing_into_the_new_field() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    late(&mut rt, FIRST, [Ime::Disabled, Ime::Enabled, Ime::Preedit("かな".into(), None),
        Ime::Preedit(String::new(), None), Ime::Commit("仮名".into())]);
    assert_eq!(texts(&rt), ("か", ""));
}

#[test]
fn a_late_preedit_never_reopens_a_composition_deactivation_finished() {
    let mut rt = composing_in_a();
    rt.window_focus(false);
    late(&mut rt, FIRST, [Ime::Preedit("かな".into(), None), Ime::Preedit(String::new(), None), Ime::Commit("仮名".into())]);
    assert_eq!(rt.state.a.text, "か", "the finished composition commits once");
    assert_eq!(rt.state.changed, ["a"]);
}

#[test]
fn a_composition_input_started_before_deactivation_finishes_with_it() {
    let mut rt = composing_in_a();
    send(&mut rt, [Ime::Preedit(String::new(), None)]);
    rt.ime(Ime::Preedit("ね".into(), None));
    rt.window_focus(false);
    rt.cancel_composition();
    rt.frame(0.0);
    assert!(rt.state.a.preedit.is_none(), "input from before the blur reopened the composition: {:?}", rt.state.a.preedit);
    assert_eq!(rt.state.a.text, "ね");
}

/// The update arrived before the blur, so the text it shows is what commits: a typed character
/// is never lost, and the conversion after the blur never lands.
#[test]
fn an_update_queued_before_deactivation_commits_and_the_late_conversion_never_lands() {
    let mut rt = composing_in_a();
    rt.ime(Ime::Preedit("かな".into(), None));
    rt.window_focus(false);
    rt.cancel_composition();
    rt.frame(0.0);
    late(&mut rt, FIRST, [Ime::Preedit(String::new(), None), Ime::Commit("仮名".into())]);
    assert_eq!(rt.state.a.text, "かな");
    assert_eq!(rt.state.changed, ["a"]);
}

#[test]
fn a_key_queued_before_deactivation_does_not_let_the_old_commit_through() {
    let mut rt = composing_in_a();
    rt.key(KeyEvent { key: Key::Tab, mods: Mods::default() });
    rt.window_focus(false);
    rt.frame(0.0);
    late(&mut rt, FIRST, [Ime::Disabled, Ime::Enabled, Ime::Preedit(String::new(), None), Ime::Commit("家".into())]);
    assert_eq!(texts(&rt), ("か", ""));
}

#[test]
fn the_ended_sessions_commit_drops_and_a_direct_commit_in_the_new_one_arrives() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    late(&mut rt, FIRST, [Ime::Commit("家".into())]);
    send(&mut rt, [Ime::Commit("，".into())]);
    assert_eq!(texts(&rt), ("か", "，"));
}

#[test]
fn hiding_finishes_the_composition_and_drops_its_late_commit() {
    let mut rt = composing_in_a();
    rt.cancel_composition();
    rt.set_live_visible(false);
    assert_eq!(rt.state.a.text, "か");
    assert_eq!(rt.state.a_focused_at_change, [true]);
    late(&mut rt, FIRST, [Ime::Disabled, Ime::Enabled, Ime::Preedit(String::new(), None), Ime::Commit("家".into())]);
    assert_eq!(rt.state.a.text, "か");
    rt.set_live_visible(true);
    send(&mut rt, [Ime::Preedit("ね".into(), None), Ime::Preedit(String::new(), None), Ime::Commit("猫".into())]);
    assert_eq!(rt.state.a.text, "か猫");
}

#[test]
fn the_apps_blur_callback_sees_the_composition_committed() {
    let mut rt = composing_in_a();
    rt.on_focus = Some(|s, focused, _| if !focused { assert_eq!(s.a.text, "か"); assert_eq!(s.changed, ["a"]); });
    rt.window_focus(false);
    assert_eq!(rt.state.a.text, "か");
}

#[test]
fn a_click_in_the_composing_field_keeps_its_composition_open() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 20.0);
    send(&mut rt, [Ime::Preedit(String::new(), None), Ime::Commit("家".into())]);
    assert_eq!(rt.state.a.text, "家");
    assert!(rt.state.b.text.is_empty());
}

#[test]
fn a_composition_over_a_selection_commits_once_and_undoes() {
    let mut rt = composing_in_a();
    rt.update(|s, _| { s.a.replace_text("before"); s.a.anchor = 0; s.a.focus = 6; });
    rt.frame(0.0);
    send(&mut rt, [Ime::Preedit("か".into(), None)]);
    click(&mut rt, 100.0, 60.0);
    assert_eq!(rt.state.a.text, "か");
    answer(&mut rt, Answer::Commit);
    assert_eq!(rt.state.a.text, "か");
    rt.update(|s, _| s.a.undo());
    assert_eq!(rt.state.a.text, "before");
}

#[test]
fn clearing_undo_history_keeps_the_composition_its_fields() {
    let mut rt = composing_in_a();
    rt.update(|s, _| s.a.clear_history());
    rt.frame(0.0);
    click(&mut rt, 100.0, 60.0);
    answer(&mut rt, Answer::Commit);
    assert_eq!(rt.state.a.text, "か", "undo history is not the composition's identity");
    assert_eq!(rt.state.changed, ["a"]);
}

/// The new rich model ends the old composition's session; the composition formatting runs
/// through is the current session's.
#[test]
fn review_formatting_the_selection_does_not_lose_a_shown_composition() {
    let mut rt = composing_in_a();
    rt.update(|s, _| { s.a = TextEdit::new("before").rich(); s.a.focused = true; s.a.anchor = 0; s.a.focus = 6; });
    rt.frame(0.0);
    send(&mut rt, [Ime::Preedit("か".into(), None)]);
    press(&mut rt, Key::Command('b'), false);
    click(&mut rt, 100.0, 60.0);
    answer(&mut rt, Answer::Commit);
    assert_eq!(rt.state.a.text, "か", "changing formatting must not change the identity of the active composition");
}

#[test]
fn review_headless_new_composition_after_silent_reset_survives() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    send(&mut rt, [Ime::Disabled, Ime::Enabled]);
    send(&mut rt, [Ime::Preedit("ね".into(), None), Ime::Commit("猫".into())]);
    assert_eq!(rt.state.b.text, "猫");
}

#[test]
fn review_silent_cancel_then_direct_commit_is_not_lost() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    send(&mut rt, [Ime::Preedit(String::new(), None), Ime::Disabled, Ime::Enabled]);
    send(&mut rt, [Ime::Commit("，".into())]);
    assert_eq!(texts(&rt), ("か", "，"));
}

#[test]
fn review_two_fast_focus_changes_preserve_new_composition() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    send(&mut rt, [Ime::Preedit("ね".into(), None)]);
    click(&mut rt, 100.0, 20.0);
    assert_eq!(rt.state.b.text, "ね");
}

#[test]
fn the_platforms_answer_to_a_reset_is_the_ended_sessions_until_its_disabled() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    rt.ime_reset_sent(true);
    platform(&mut rt, [Ime::Preedit(String::new(), None), Ime::Commit("家".into()), Ime::Disabled]);
    platform(&mut rt, [Ime::Enabled, Ime::Preedit("ね".into(), None), Ime::Preedit(String::new(), None), Ime::Commit("猫".into())]);
    assert_eq!(texts(&rt), ("か", "猫"));
}

#[test]
fn an_answer_without_its_disabled_ends_by_the_second_idle_wait() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    rt.ime_reset_sent(true);
    platform(&mut rt, [Ime::Commit("家".into())]);
    rt.ime_platform_idle();
    platform(&mut rt, [Ime::Commit("仮名".into())]);
    rt.ime_platform_idle();
    platform(&mut rt, [Ime::Commit("，".into())]);
    assert_eq!(texts(&rt), ("か", "，"));
}

#[test]
fn a_reset_that_ended_no_session_leaves_every_platform_event_current() {
    let mut rt = composing_in_a();
    send(&mut rt, [Ime::Preedit(String::new(), None), Ime::Commit("家".into())]);
    click(&mut rt, 100.0, 60.0);
    rt.ime_reset_sent(true);
    platform(&mut rt, [Ime::Commit("，".into()), Ime::Disabled]);
    assert_eq!(texts(&rt), ("家", "，"));
}

#[test]
fn a_late_wayland_batch_is_the_ended_sessions_and_a_current_one_arrives() {
    let mut rt = composing_in_a();
    click(&mut rt, 100.0, 60.0);
    rt.ime_reset_sent(false);
    rt.ime_from_platform(Ime::Preedit(String::new(), None), true);
    rt.ime_from_platform(Ime::Commit("家".into()), true);
    rt.ime_from_platform(Ime::Commit("，".into()), false);
    rt.frame(0.0);
    assert_eq!(texts(&rt), ("か", "，"));
}

/// The shell resets at once while the finish waits behind the input queued first: the answer,
/// stamped with the session that was current, still arrives after the finish ended it.
#[test]
fn the_answer_to_a_deactivation_drops_though_its_finish_waits_for_queued_input() {
    let mut rt = composing_in_a();
    rt.ime(Ime::Preedit("かな".into(), None));
    rt.window_focus(false);
    rt.ime_reset_sent(true);
    platform(&mut rt, [Ime::Preedit(String::new(), None), Ime::Commit("仮名".into()), Ime::Disabled]);
    assert_eq!(rt.state.a.text, "かな");
    assert_eq!(rt.state.changed, ["a"]);
}

#[test]
fn a_close_request_asks_the_app_with_the_composition_committed() {
    let mut rt = composing_in_a();
    rt.on_close_request(|s, request, _| { s.changed.push(if s.a.text == "か" { "saved か" } else { "saved without it" }); request.proceed(); });
    assert_eq!(rt.request_close(crate::CloseKind::Window), crate::CloseAnswer::Proceed);
    assert_eq!(rt.state.changed, ["a", "saved か"]);
}

#[test]
fn a_close_request_waits_for_input_queued_before_it_then_commits() {
    let mut rt = composing_in_a();
    rt.on_close_request(|s, request, _| { s.changed.push(if s.a.text == "かな" { "saved かな" } else { "saved early" }); request.proceed(); });
    rt.ime(Ime::Preedit("かな".into(), None));
    assert_eq!(rt.request_close(crate::CloseKind::Window), crate::CloseAnswer::Pending);
    rt.frame(0.0);
    assert_eq!(rt.settle_close(crate::CloseKind::Window), Some(crate::CloseAnswer::Proceed));
    assert_eq!(rt.state.changed, ["a", "saved かな"]);
}
