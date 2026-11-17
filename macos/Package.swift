// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "TokemonMenuBar",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "TokemonMenuBar", targets: ["TokemonMenuBar"]),
    ],
    targets: [
        .executableTarget(name: "TokemonMenuBar"),
        .testTarget(name: "TokemonMenuBarTests", dependencies: ["TokemonMenuBar"]),
    ],
    swiftLanguageModes: [.v5]
)
