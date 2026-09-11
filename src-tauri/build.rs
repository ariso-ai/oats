fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        recorder_pill::build_and_link();
    }
    tauri_build::build()
}

/// Compiles the Swift recorder pill (`recorder-pill/macos`) into a static
/// library and links it, plus the Swift runtime it needs, into this crate.
mod recorder_pill {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// Matches `bundle.macOS.minimumSystemVersion` in tauri.conf.json.
    const MIN_MACOS: &str = "14.4";
    const PRODUCT: &str = "OatsRecorderPill";

    pub fn build_and_link() {
        let manifest_dir = PathBuf::from(env("CARGO_MANIFEST_DIR"));
        let package = manifest_dir.join("recorder-pill").join("macos");
        println!("cargo:rerun-if-changed={}", package.join("Package.swift").display());
        println!("cargo:rerun-if-changed={}", package.join("Sources").display());

        let arch = match env("CARGO_CFG_TARGET_ARCH").as_str() {
            "aarch64" => "arm64",
            other => panic!("recorder pill: unsupported macOS arch {other}"),
        };
        let triple = format!("{arch}-apple-macosx{MIN_MACOS}");
        let config = if env("PROFILE") == "release" { "release" } else { "debug" };
        // Keep SwiftPM's build products out of the source tree.
        let scratch = PathBuf::from(env("OUT_DIR")).join("recorder-pill-swift");

        let swift_build = |extra: &[&str]| {
            let mut cmd = Command::new("swift");
            cmd.arg("build")
                .arg("--package-path")
                .arg(&package)
                .arg("--scratch-path")
                .arg(&scratch)
                .args(["-c", config, "--triple", &triple])
                .args(extra);
            cmd
        };
        run(&mut swift_build(&[]), "swift build");
        let bin_path = capture(&mut swift_build(&["--show-bin-path"]), "swift build --show-bin-path");
        let bin_path = Path::new(bin_path.trim());
        assert!(
            bin_path.join(format!("lib{PRODUCT}.a")).exists(),
            "recorder pill: lib{PRODUCT}.a missing from {}",
            bin_path.display()
        );
        println!("cargo:rustc-link-search=native={}", bin_path.display());
        println!("cargo:rustc-link-lib=static={PRODUCT}");

        // The Swift objects autolink swiftCore, Foundation, AppKit, SwiftUI, …
        // via LC_LINKER_OPTION; the linker only needs to be able to find them.
        let info = capture(
            Command::new("swift").args(["-print-target-info", "-target", &triple]),
            "swift -print-target-info",
        );
        let info: serde_json::Value =
            serde_json::from_str(&info).expect("recorder pill: unparseable swift target info");
        for path in info["paths"]["runtimeLibraryPaths"]
            .as_array()
            .expect("recorder pill: no runtimeLibraryPaths in swift target info")
        {
            println!("cargo:rustc-link-search=native={}", path.as_str().unwrap());
        }
        let sdk = capture(
            Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]),
            "xcrun --show-sdk-path",
        );
        println!("cargo:rustc-link-search=native={}/usr/lib/swift", sdk.trim());
        // The OS copy of the Swift runtime (in the dyld shared cache).
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }

    fn env(key: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| panic!("recorder pill: {key} not set"))
    }

    fn run(cmd: &mut Command, what: &str) {
        let status = cmd
            .status()
            .unwrap_or_else(|e| panic!("recorder pill: failed to launch `{what}`: {e}"));
        assert!(status.success(), "recorder pill: `{what}` failed ({status})");
    }

    fn capture(cmd: &mut Command, what: &str) -> String {
        let out = cmd
            .output()
            .unwrap_or_else(|e| panic!("recorder pill: failed to launch `{what}`: {e}"));
        assert!(
            out.status.success(),
            "recorder pill: `{what}` failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).expect("recorder pill: non-UTF-8 tool output")
    }
}
