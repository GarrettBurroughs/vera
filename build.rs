use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=selftests");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_selftests.rs");

    let mut out = String::new();

    let mut entries: Vec<_> = fs::read_dir("selftests")
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("vera"))
        .collect();

    // Sort for deterministic test ordering.
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let path = entry.path();
        let name = path
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap()
            .replace("-", "_")
            .replace(".", "_");
        
        let path_str = path.display().to_string().replace("\\", "/");

        out.push_str(&format!(
            "#[test]\nfn test_{}() {{\n    run_single_test(\"{}\");\n}}\n\n",
            name, path_str
        ));
    }

    fs::write(&dest_path, out).unwrap();
}
