// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Cache",
    products: [.library(name: "Cache", targets: ["Cache"])],
    targets: [
        .target(name: "Cache"),
        .testTarget(name: "CacheTests", dependencies: ["Cache"]),
    ]
)
