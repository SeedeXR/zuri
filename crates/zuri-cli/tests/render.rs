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

#[test]
fn a_lottie_file_is_imported_beside_itself_and_what_is_left_out_is_said() {
    // A Lottie file is `.json`, as the source made of it is: without `-o` that goes to
    // `<name>.zuri.json`, and the file stays as it was.
    let dir = concat!(env!("CARGO_TARGET_TMPDIR"), "/lottie");
    std::fs::create_dir_all(dir).unwrap();
    let lottie = r#"{ "fr": 30, "ip": 0, "op": 30, "w": 10, "h": 10, "layers": [ { "ty": 5, "ip": 0, "op": 30 } ] }"#;
    std::fs::write(format!("{dir}/words.json"), lottie).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_zuri")).args(["import", &format!("{dir}/words.json"), "--json"]).output().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!((report["ok"].as_bool(), report["notes"][0].as_str()), (Some(true), Some("text layers")), "{report}");
    assert!(report["output"].as_str().unwrap().ends_with("words.zuri.json"), "{report}");
    assert_eq!(std::fs::read_to_string(format!("{dir}/words.json")).unwrap(), lottie);
    let built = Command::new(env!("CARGO_BIN_EXE_zuri")).args(["build", report["output"].as_str().unwrap()]).output().unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
}
