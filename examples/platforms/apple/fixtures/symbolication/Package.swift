// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Symbolication",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "Symbolication", targets: ["Symbolication"])],
    dependencies: [.package(path: "ffi")],
    targets: [
        .executableTarget(
            name: "Symbolication",
            dependencies: [.product(name: "DemoFFI", package: "ffi")]
        ),
    ]
)
