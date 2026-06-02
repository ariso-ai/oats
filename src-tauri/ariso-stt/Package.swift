// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ariso-stt",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(name: "ariso-stt")
    ]
)
