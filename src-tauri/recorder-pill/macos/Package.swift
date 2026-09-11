// swift-tools-version:5.9
// The floating recorder pill, drawn natively with AppKit + SwiftUI. Built as a
// static library by `src-tauri/build.rs` and linked into the oats binary; the
// Rust side drives it through the C entry points in `Bridge.swift`.
import PackageDescription

let package = Package(
    name: "OatsRecorderPill",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "OatsRecorderPill", type: .static, targets: ["OatsRecorderPill"]),
    ],
    targets: [
        .target(name: "OatsRecorderPill"),
        .testTarget(name: "OatsRecorderPillTests", dependencies: ["OatsRecorderPill"]),
    ]
)
