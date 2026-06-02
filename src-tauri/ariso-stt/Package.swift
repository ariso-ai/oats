// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ariso-stt",
    platforms: [.macOS(.v14)],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", from: "0.14.8")
    ],
    targets: [
        .executableTarget(
            name: "ariso-stt",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio")
            ]
        )
    ]
)
