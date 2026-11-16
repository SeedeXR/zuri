//! `zuri render --set`, through the built command.

use std::process::Command;

#[test]
fn set_writes_into_a_list_that_an_earlier_set_made_longer() {
    // examples/rows.json: a Row repeated `items` (3) times as `rows`. The list is as long
    // as `items` says as soon as it is set, so the next --set can write to a new copy.
    let scene = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/rows.json");
    let png = concat!(env!("CARGO_TARGET_TMPDIR"), "/rows.png");
    let run = |sets: &[&str]| {
        let sets = sets.iter().flat_map(|s| ["--set", s]);
        let out = Command::new(env!("CARGO_BIN_EXE_zuri")).args(["render", scene, "-o", png]).args(sets).output().unwrap();
        (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
    };
    assert_eq!(run(&["rows/2/title=Third"]), (true, String::new()));
    assert_eq!(run(&["items=6", "rows/5/title=Sixth"]), (true, String::new()));
    let (ok, why) = run(&["items=6", "rows/6/title=Seventh"]);
    assert!(!ok && why.contains(r#"no view-model property "rows/6/title""#), "{why}");
}
