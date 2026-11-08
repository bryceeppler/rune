use super::*;

const LAPTOP: Screen = Screen { area: [0, 25, 1512, 957], units: 1.0, slack: 0, primary: true };
const MONITOR: Screen = Screen { area: [1512, -300, 2560, 1415], units: 1.0, slack: 0, primary: false };

/// Where a window saved with `frame`, in points, opens on `screens`.
fn fit(frame: [i32; 4], screens: &[Screen]) -> [i32; 4] {
    WindowGeometry { frame: Some(frame), scale: 1.0, ..Default::default() }.fitted(screens, 1.0).frame.unwrap()
}

#[test]
fn a_frame_on_a_present_screen_keeps_its_place() {
    assert_eq!(fit([200, 100, 900, 600], &[LAPTOP, MONITOR]), [200, 100, 900, 600]);
    assert_eq!(fit([1700, -200, 1200, 800], &[LAPTOP, MONITOR]), [1700, -200, 1200, 800]);
}

#[test]
fn a_frame_from_a_display_that_is_gone_centres_on_the_primary_screen() {
    let placed = fit([3000, 200, 1200, 800], &[LAPTOP]);
    assert_eq!(placed, [156, 103, 1200, 800]);
    assert!(within(placed, LAPTOP.area));
}

#[test]
fn a_frame_hanging_off_its_screen_moves_wholly_onto_it() {
    let placed = fit([1300, 800, 900, 600], &[LAPTOP]);
    assert_eq!(placed, [612, 382, 900, 600]);
    let straddling = fit([1400, 100, 900, 600], &[LAPTOP, MONITOR]);
    assert_eq!(straddling, [1512, 100, 900, 600], "onto the screen holding most of it");
}

#[test]
fn a_frame_larger_than_its_screen_shrinks_to_it() {
    assert_eq!(fit([-50, 0, 2000, 1200], &[LAPTOP]), [0, 25, 1512, 957]);
}

#[test]
fn a_windows_border_may_pass_the_work_area_by_the_slack() {
    let screen = Screen { area: [0, 0, 1920, 1040], units: 1.0, slack: 7, primary: true };
    assert_eq!(fit([-7, 0, 1100, 700], &[screen]), [-7, 0, 1100, 700]);
    assert_eq!(fit([-30, 0, 1100, 700], &[screen]), [-7, 0, 1100, 700]);
}

#[test]
fn no_screens_leaves_the_frame_as_saved() {
    assert_eq!(fit([0, 0, 10, 10], &[]), [0, 0, 10, 10]);
}

#[test]
fn a_fitted_geometry_shrinks_its_content_by_what_the_frame_lost() {
    let saved = WindowGeometry { frame: Some([0, 0, 2000, 1000]), size: (1000.0, 486.0), scale: 2.0, ..Default::default() };
    let placed = saved.fitted(&[Screen { area: [0, 0, 1600, 1000], units: 2.0, slack: 0, primary: true }], 2.0);
    assert_eq!((placed.frame, placed.size), (Some([0, 0, 1600, 1000]), (800.0, 486.0)));
    let wayland = WindowGeometry { frame: None, ..saved.clone() };
    assert_eq!(wayland.fitted(&[LAPTOP], 1.0), wayland, "without a frame only the size restores");
}

#[test]
fn a_frame_saved_at_one_scale_fits_a_screen_at_another() {
    // Saved at 1 pixel a point on a monitor that is gone; the screen left has 2.
    let saved = WindowGeometry { frame: Some([2400, 0, 600, 400]), size: (600.0, 400.0), scale: 1.0, ..Default::default() };
    let screen = Screen { area: [0, 0, 800, 600], units: 2.0, slack: 0, primary: true };
    let placed = saved.fitted(&[screen], 1.0);
    assert_eq!((placed.frame, placed.size), (Some([0, 0, 800, 600]), (400.0, 300.0)));
    let opened = [0, 0, (placed.size.0 * 2.0) as i32, (placed.size.1 * 2.0) as i32];
    assert!(within(opened, screen.area), "the window it opens, {opened:?}, lies on the screen");
    let roomy = Screen { area: [0, 0, 2000, 1600], ..screen };
    let placed = saved.fitted(&[roomy], 1.0);
    assert_eq!((placed.frame, placed.size), (Some([400, 400, 1200, 800]), (600.0, 400.0)), "it keeps its size in points");
}

fn within(frame: [i32; 4], area: [i32; 4]) -> bool {
    let ([x, y, w, h], [ax, ay, aw, ah]) = (frame.map(i64::from), area.map(i64::from));
    x >= ax && y >= ay && x + w <= ax + aw && y + h <= ay + ah
}

const EXTREMES: [i32; 9] = [i32::MIN, i32::MIN + 1, -100_000, -1, 0, 1, 700, 100_000, i32::MAX];

#[test]
fn any_saved_frame_fits_wholly_on_a_present_screen() {
    let windows = Screen { area: [-1920, 0, 1920, 1040], units: 1.5, slack: 12, primary: false };
    let layouts: [&[Screen]; 3] = [&[LAPTOP], &[LAPTOP, MONITOR], &[windows, LAPTOP]];
    for screens in layouts {
        for x in EXTREMES { for y in EXTREMES { for w in EXTREMES { for h in EXTREMES {
            let saved = WindowGeometry { frame: Some([x, y, w, h]), size: (800.0, 600.0), scale: 1.0, ..Default::default() };
            let placed = std::panic::catch_unwind(|| saved.fitted(screens, 1.0));
            let Ok(placed) = placed else { panic!("fitting {:?} panicked", [x, y, w, h]) };
            let frame = placed.frame.unwrap();
            let home = screens.iter().find(|s| within(frame, grown(s)));
            assert!(home.is_some(), "{:?} fitted to {frame:?}, off every screen", [x, y, w, h]);
            assert!(frame[2] > 0 && frame[3] > 0, "{:?} fitted to the empty frame {frame:?}", [x, y, w, h]);
            assert!(placed.size.0 > 0.0 && placed.size.1 > 0.0, "{:?} opens with content {:?}", [x, y, w, h], placed.size);
        }}}}
    }
}

fn grown(s: &Screen) -> [i32; 4] { [s.area[0] - s.slack, s.area[1] - s.slack, s.area[2] + 2 * s.slack, s.area[3] + 2 * s.slack] }
