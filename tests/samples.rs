use siga::spec::Spec;

#[test]
fn every_sample_loads() {
    let mut n = 0;
    for dir in std::fs::read_dir("samples").unwrap() {
        for file in std::fs::read_dir(dir.unwrap().path()).unwrap() {
            let path = file.unwrap().path();
            if !matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("yaml" | "yml" | "json")
            ) {
                continue;
            }
            if let Err(e) = Spec::load(&path) {
                panic!("{}: {e}", path.display());
            }
            n += 1;
        }
    }
    assert!(n > 0, "no sample specs found");
}
