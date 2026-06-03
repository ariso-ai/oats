// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ariso-stt",
    platforms: [.macOS(.v14)],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", from: "0.14.8"),
        .package(url: "https://github.com/ml-explore/mlx-swift-lm.git", from: "2.30.0")
    ],
    targets: [
        .executableTarget(
            name: "ariso-stt",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio"),
                .product(name: "MLXLLM", package: "mlx-swift-lm"),
                .product(name: "MLXLMCommon", package: "mlx-swift-lm")
            ]
        )
    ]
)
