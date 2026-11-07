use std::cell::RefCell;
use std::rc::Rc;

use super::*;

/// A window whose hook answers `now`, then `later` once its save ends.
#[derive(Default)]
struct Hook {
    now: Option<CloseAnswer>,
    later: Option<CloseAnswer>,
    asked: usize,
    withdrawn: usize,
    closed: bool,
}

struct Fake(Rc<RefCell<Hook>>);

impl AnyHost for Fake {
    fn resumed(&mut self, _: &ActiveEventLoop) {}
    fn user_event(&mut self, _: &ActiveEventLoop, _: WakeEvent) {}
    fn owns(&self, _: WindowId) -> bool { false }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    fn terminate(&mut self) { self.0.borrow_mut().closed = true; }
    fn ask_quit(&mut self) -> CloseAnswer {
        let mut hook = self.0.borrow_mut();
        hook.asked += 1;
        hook.now.unwrap_or(CloseAnswer::Pending)
    }
    fn settle_quit(&mut self) -> Option<CloseAnswer> {
        let hook = self.0.borrow();
        Some(hook.now.or(hook.later).unwrap_or(CloseAnswer::Pending))
    }
    fn withdraw_quit(&mut self) { self.0.borrow_mut().withdrawn += 1; }
}

fn windows(answers: &[Option<CloseAnswer>]) -> (Vec<Option<Box<dyn AnyHost>>>, Vec<Rc<RefCell<Hook>>>) {
    let hooks: Vec<_> = answers.iter().map(|&now| Rc::new(RefCell::new(Hook { now, ..Hook::default() }))).collect();
    (hooks.iter().map(|h| Some(Box::new(Fake(h.clone())) as Box<dyn AnyHost>)).collect(), hooks)
}

use CloseAnswer::{Cancel, Proceed};

#[test]
fn a_session_query_every_window_approves_closes_them_before_it_answers() {
    let (mut hosts, hooks) = windows(&[Some(Proceed), Some(Proceed)]);
    let mut quitting = None;
    assert_eq!(session(&mut hosts, &mut quitting, SessionStep::Query), SessionReply::Ends);
    assert!(quitting.is_none() && hooks.iter().all(|h| h.borrow().asked == 1));
    assert!(hooks.iter().all(|h| h.borrow().closed), "on_close runs inside the query, not at a WM_ENDSESSION that may come too late");
}

#[test]
fn input_after_a_window_proceeds_keeps_the_quit_waiting_for_it() {
    let (mut hosts, hooks) = windows(&[Some(Proceed), None]);
    let mut quit = Quitting::ask(&mut hosts, QuitFrom::Rune);
    assert_eq!(quit.settle(&mut hosts), CloseAnswer::Pending, "Settings is saving");
    // Input reached the note after it proceeded, so its request is asked again and holds.
    hooks[0].borrow_mut().now = None;
    hooks[1].borrow_mut().later = Some(Proceed);
    assert_eq!(quit.settle(&mut hosts), CloseAnswer::Pending, "the note's earlier proceed no longer counts");
    hooks[0].borrow_mut().later = Some(Proceed);
    assert_eq!(quit.settle(&mut hosts), Proceed);
}

#[test]
fn a_window_saving_makes_the_session_wait_until_it_answers() {
    let (mut hosts, hooks) = windows(&[Some(Proceed), None]);
    let mut quitting = None;
    assert_eq!(session(&mut hosts, &mut quitting, SessionStep::Query), SessionReply::Waits);
    assert!(quitting.as_ref().is_some_and(|q| q.from == QuitFrom::Session), "the loop finishes the quit when the save ends");
    hooks[1].borrow_mut().later = Some(Proceed);
    assert_eq!(quitting.as_mut().unwrap().settle(&mut hosts), Proceed);
    assert_eq!(hooks[0].borrow().asked, 1, "asked once");
}

#[test]
fn a_window_that_cancels_refuses_the_session_end() {
    let (mut hosts, hooks) = windows(&[Some(Proceed), Some(Cancel)]);
    let mut quitting = None;
    assert_eq!(session(&mut hosts, &mut quitting, SessionStep::Query), SessionReply::Refuses);
    assert!(quitting.is_none() && hooks.iter().all(|h| h.borrow().withdrawn == 1));
}

#[test]
fn the_session_end_runs_every_close_hook_even_while_a_save_runs() {
    let (mut hosts, hooks) = windows(&[Some(Proceed), None]);
    let mut quitting = None;
    session(&mut hosts, &mut quitting, SessionStep::Query);
    session(&mut hosts, &mut quitting, SessionStep::Ends);
    assert!(quitting.is_none() && hooks.iter().all(|h| h.borrow().closed));
}

#[test]
fn a_session_that_goes_on_forgets_its_query_but_not_the_app_s_own_quit() {
    let (mut hosts, hooks) = windows(&[None]);
    let mut quitting = None;
    session(&mut hosts, &mut quitting, SessionStep::Query);
    session(&mut hosts, &mut quitting, SessionStep::Continues);
    assert!(quitting.is_none() && hooks[0].borrow().withdrawn == 1);
    let mut quitting = Some(Quitting::ask(&mut hosts, QuitFrom::Rune));
    session(&mut hosts, &mut quitting, SessionStep::Continues);
    assert!(quitting.is_some());
}

#[test]
fn the_last_main_window_asks_the_rest_and_again_each_time_fewer_show() {
    let mut last = LastMain::default();
    assert!(!last.ask(true, 2), "a main window keeps the app");
    assert!(last.ask(false, 1), "its close asks the windows left");
    assert!(!last.ask(false, 1), "once");
    assert!(last.ask(false, 0), "again when the one that refused goes");
    assert!(!last.ask(true, 1) && last.ask(false, 1), "a main window that opens and closes asks afresh");
}
