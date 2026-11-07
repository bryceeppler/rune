use super::*;
use crate::el::el;
use crate::runtime::{Cx, Env};

#[derive(Default)]
struct Notes {
    /// What the hook does with a request: `Some(true)` proceeds, `Some(false)` cancels, None keeps it.
    reply: Option<bool>,
    asked: Vec<CloseKind>,
    held: Vec<CloseRequest>,
    text: String,
    saved: String,
}

fn hook(n: &mut Notes, request: CloseRequest, _: &mut Tx) {
    n.asked.push(request.kind());
    n.saved = n.text.clone();
    match n.reply {
        Some(true) => request.proceed(),
        Some(false) => request.cancel(),
        None => n.held.push(request),
    }
}

fn runtime(reply: Option<bool>) -> Runtime<Notes> {
    let mut rt = Runtime::new(Notes { reply, ..Notes::default() }, |_: &Notes, _: &mut Cx<Notes>| el(), (100.0, 100.0), Env::default());
    rt.on_close_request(hook);
    rt.on_key = Some(|n, e, _| { if let crate::Key::Text(t) = &e.key { n.text.push_str(t) } true });
    rt.frame(0.0);
    rt
}

fn type_text(rt: &mut Runtime<Notes>, text: &str) {
    rt.key(crate::KeyEvent { key: crate::Key::Text(text.into()), mods: Default::default() });
}

#[test]
fn a_window_without_a_hook_closes_at_once() {
    let mut rt = Runtime::new(0u32, |_: &u32, _: &mut Cx<u32>| el(), (100.0, 100.0), Env::default());
    assert_eq!(rt.request_close(CloseKind::Window), CloseAnswer::Proceed);
    assert_eq!(rt.settle_close(CloseKind::Window), None);
}

#[test]
fn the_hook_answers_now_or_never() {
    let mut rt = runtime(Some(true));
    assert_eq!(rt.request_close(CloseKind::Quit), CloseAnswer::Proceed);
    let mut rt = runtime(Some(false));
    assert_eq!(rt.request_close(CloseKind::Window), CloseAnswer::Cancel);
    assert_eq!(rt.state.asked, [CloseKind::Window]);
    assert_eq!(rt.settle_close(CloseKind::Window), None, "an answer given at once is not kept in flight");
}

#[test]
fn a_kept_request_proceeds_later_from_another_thread_and_wakes_the_window() {
    let mut rt = runtime(None);
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = woken.clone();
    rt.closing.set_wake(Arc::new(move || { count.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }));
    assert_eq!(rt.request_close(CloseKind::Window), CloseAnswer::Pending);
    assert_eq!(rt.settle_close(CloseKind::Window), Some(CloseAnswer::Pending));
    assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), 1, "asking wakes the window to carry out what the hook asked");
    let request = rt.state.held.pop().unwrap();
    std::thread::spawn(move || request.proceed()).join().unwrap();
    assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(rt.settle_close(CloseKind::Window), Some(CloseAnswer::Proceed));
    assert_eq!(rt.settle_close(CloseKind::Window), None, "settling takes the answer");
}

#[test]
fn a_dropped_request_cancels() {
    let mut rt = runtime(None);
    rt.request_close(CloseKind::Quit);
    rt.state.held.clear();
    assert_eq!(rt.settle_close(CloseKind::Quit), Some(CloseAnswer::Cancel));
}

#[test]
fn a_repeated_request_joins_the_pending_one_and_asks_again_once_it_is_answered() {
    let mut rt = runtime(None);
    rt.request_close(CloseKind::Quit);
    assert_eq!(rt.request_close(CloseKind::Quit), CloseAnswer::Pending);
    assert_eq!(rt.state.asked, [CloseKind::Quit], "the second quit joins the first");
    rt.request_close(CloseKind::Window);
    assert_eq!(rt.state.asked, [CloseKind::Quit, CloseKind::Window], "each kind has its own request");
    rt.state.held.remove(0).cancel();
    assert_eq!(rt.settle_close(CloseKind::Quit), Some(CloseAnswer::Cancel));
    assert_eq!(rt.request_close(CloseKind::Quit), CloseAnswer::Pending);
    assert_eq!(rt.state.asked.len(), 3, "once settled, the next quit asks afresh");
}

#[test]
fn a_withdrawn_request_ignores_its_late_answer() {
    let mut rt = runtime(None);
    rt.request_close(CloseKind::Quit);
    rt.withdraw_close(CloseKind::Quit);
    rt.state.held.pop().unwrap().proceed();
    assert_eq!(rt.settle_close(CloseKind::Quit), None);
}

#[test]
fn input_queued_when_a_quit_arrives_reaches_the_app_before_the_hook() {
    let mut rt = runtime(Some(true));
    type_text(&mut rt, "x");
    assert_eq!(rt.request_close(CloseKind::Quit), CloseAnswer::Pending);
    assert!(rt.state.asked.is_empty(), "the hook waits for the frame that delivers the key");
    rt.frame(0.016);
    assert_eq!(rt.state.saved, "x");
    assert_eq!(rt.settle_close(CloseKind::Quit), Some(CloseAnswer::Proceed));
}

#[test]
fn input_given_while_a_save_runs_asks_again_before_the_window_goes() {
    let mut rt = runtime(None);
    assert_eq!(rt.request_close(CloseKind::Window), CloseAnswer::Pending);
    type_text(&mut rt, "x");
    // The save started before the key finishes in the frame that delivers it.
    rt.poll = Some(|n, _, _| { if let Some(r) = n.held.pop() { r.proceed() } false });
    rt.frame(0.016);
    assert_eq!(rt.state.text, "x");
    assert_eq!(rt.settle_close(CloseKind::Window), Some(CloseAnswer::Pending), "a proceed given before the key is stale");
    assert_eq!((rt.state.asked.len(), rt.state.saved.as_str()), (2, "x"), "the hook is asked again with the key");
    rt.frame(0.032);
    assert_eq!(rt.settle_close(CloseKind::Window), Some(CloseAnswer::Proceed));
}

#[test]
fn a_quit_waits_for_every_window_and_any_cancel_ends_it() {
    use CloseAnswer::*;
    assert_eq!(all_of([]), Proceed);
    assert_eq!(all_of([Proceed, Pending]), Pending);
    assert_eq!(all_of([Pending, Cancel, Proceed]), Cancel);
    assert_eq!(all_of([Proceed, Proceed]), Proceed);
}
