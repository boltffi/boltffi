// swift-tools-version:{{ manifest.tools_version }}
import PackageDescription

let releaseTag = "{{ version }}"
let releaseChecksum = "{{ checksum }}"

let package = Package(
    name: "{{ manifest.package_name }}",
    platforms: [{% for platform in manifest.platform_declarations %}
        {{ platform }},{% endfor %}
    ],
    products: [
        .library(
            name: "{{ manifest.package_name }}",
            targets: ["{{ manifest.product_target_name }}"]
        ),
    ],
    targets: [
        .binaryTarget(
            name: "{{ manifest.binary_target_name }}",
            url: "{{ repo_url }}/releases/download/\(releaseTag)/{{ manifest.xcframework_name }}.xcframework.zip",
            checksum: releaseChecksum
        ),
        .target(
            name: "{{ manifest.ffi_module_name }}",
            dependencies: ["{{ manifest.binary_target_name }}"],
            path: "FFI",
            publicHeadersPath: "include"
        ){% if manifest.has_wrapper_target %},
        .target(
            name: "{{ manifest.module_name }}",
            dependencies: ["{{ manifest.ffi_module_name }}"],
            path: "{{ manifest.wrapper_sources }}"
        ){% endif %},
    ]
)
