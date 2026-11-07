use super::*;

const LAPTOP: Screen = Screen { area: [0, 25, 1512, 957], primary: true };
const MONITOR: Screen = Screen { area: [1512, -300, 2560, 1415], primary: false };

#[test]
fn a_frame_on_a_present_screen_keeps_its_place() {
    assert_eq!(fit([200, 100, 900, 600], &[LAPTOP, MONITOR], 0), Some([200, 100, 900, 600]));
    assert_eq!(fit([1700, -200, 1200, 800], &[LAPTOP, MONITOR], 0), Some([1700, -200, 1200, 800]));
}

#[test]
fn a_frame_from_a_display_that_is_gone_centres_on_the_primary_screen() {
    let placed = fit([3000, 200, 1200, 800], &[LAPTOP], 0);
    assert_eq!(placed, Some([156, 103, 1200, 800]));
    assert!(within(placed.unwrap(), LAPTOP.area));
}

#[test]
fn a_frame_hanging_off_its_screen_moves_wholly_onto_it() {
    let placed = fit([1300, 800, 900, 600], &[LAPTOP], 0).unwrap();
    assert_eq!(placed, [612, 382, 900, 600]);
    let straddling = fit([1400, 100, 900, 600], &[LAPTOP, MONITOR], 0).unwrap();
    assert_eq!(straddling, [1512, 100, 900, 600], "onto the screen holding most of it");
}

#[test]
fn a_frame_larger_than_its_screen_shrinks_to_it() {
    assert_eq!(fit([-50, 0, 2000, 1200], &[LAPTOP], 0), Some([0, 25, 1512, 957]));
}

#[test]
fn a_windows_border_may_pass_the_work_area_by_the_slack() {
    let screen = Screen { area: [0, 0, 1920, 1040], primary: true };
    assert_eq!(fit([-7, 0, 1100, 700], &[screen], 7), Some([-7, 0, 1100, 700]));
    assert_eq!(fit([-30, 0, 1100, 700], &[screen], 7), Some([-7, 0, 1100, 700]));
}

#[test]
fn no_screens_leaves_nothing_to_fit_to() {
    assert_eq!(fit([0, 0, 10, 10], &[], 0), None);
}

#[test]
fn a_fitted_geometry_shrinks_its_content_by_what_the_frame_lost() {
    let saved = WindowGeometry { frame: Some([0, 0, 2000, 1000]), size: (1000.0, 486.0), scale: 2.0, ..Default::default() };
    let placed = saved.fitted(&[Screen { area: [0, 0, 1600, 1000], primary: true }], 0, 2.0);
    assert_eq!((placed.frame, placed.size), (Some([0, 0, 1600, 1000]), (800.0, 486.0)));
    let wayland = WindowGeometry { frame: None, ..saved.clone() };
    assert_eq!(wayland.fitted(&[LAPTOP], 0, 1.0), wayland, "without a frame only the size restores");
}

fn within(frame: [i32; 4], area: [i32; 4]) -> bool {
    frame[0] >= area[0] && frame[1] >= area[1] && frame[0] + frame[2] <= area[0] + area[2] && frame[1] + frame[3] <= area[1] + area[3]
}
