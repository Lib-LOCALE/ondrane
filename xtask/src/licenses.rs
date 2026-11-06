//! Assemble upstream notices and MPL-covered source alongside binary distributions.
use std::{fs, path::Path, process::Command};

pub fn generate(root: &Path) {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(root)
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()
        .expect("cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("metadata JSON");
    let destination = root.join("target/third-party-licenses");
    fs::create_dir_all(&destination).unwrap();
    let mut inventory = String::from("name,version,declared_license,repository\n");
    let mut count = 0;
    for package in metadata["packages"].as_array().unwrap() {
        if package["source"].is_null() {
            continue;
        }
        let name = package["name"].as_str().unwrap();
        let version = package["version"].as_str().unwrap();
        let license = package["license"]
            .as_str()
            .expect("dependency without declared licence");
        let repository = package["repository"].as_str().unwrap_or("");
        inventory.push_str(
            &[name, version, license, repository]
                .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
                .join(","),
        );
        inventory.push('\n');
        let source = Path::new(package["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap();
        let target = destination.join(format!("{name}-{version}"));
        fs::create_dir_all(&target).unwrap();
        let mut copied = 0;
        for entry in fs::read_dir(source).unwrap().flatten() {
            let path = entry.path();
            let filename = entry.file_name().to_string_lossy().to_string();
            let lower = filename.to_lowercase();
            if [
                "license",
                "licence",
                "copying",
                "copyright",
                "notice",
                "unlicense",
            ]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
            {
                if path.is_dir() {
                    super::copy_dir_all(&path, &target.join(filename));
                } else {
                    super::copy(&path, &target.join(filename));
                }
                copied += 1;
            }
        }
        if let Some(file) = package["license_file"].as_str() {
            super::copy(&source.join(file), &target.join("DECLARED-LICENSE"));
            copied += 1;
        }
        // Some permissive crates put their grant only in README/source headers.
        if copied == 0 {
            for filename in ["README.md", "README", "src/lib.rs"] {
                if source.join(filename).is_file() {
                    super::copy(
                        &source.join(filename),
                        &target.join(filename.replace('/', "-")),
                    );
                    copied += 1;
                }
            }
        }
        assert!(copied > 0, "no notice material found for {name} {version}");
        if name == "option-ext" {
            super::copy_dir_all(&source.join("src"), &target.join("source/src"));
            super::copy(
                &source.join("Cargo.toml"),
                &target.join("source/Cargo.toml"),
            );
            super::copy(
                &source.join("LICENSE.txt"),
                &target.join("source/LICENSE.txt"),
            );
        }
        count += 1;
    }
    fs::write(destination.join("RUST-DEPENDENCIES.csv"), inventory).unwrap();
    super::copy_dir_all(
        &root.join("assets/theme-licenses"),
        &destination.join("themes"),
    );
    super::copy(
        &root.join("ondrane/ui/assets/fonts/LICENSE-Inter.txt"),
        &destination.join("Inter.txt"),
    );
    super::copy(
        &root.join("THIRD_PARTY_NOTICES.md"),
        &destination.join("README.md"),
    );
    eprintln!(
        "Collected notices for {count} resolved crates, Inter and colour palettes in {}",
        destination.display()
    );
}
