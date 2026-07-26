import Foundation
import XCTest

private struct RustExportInventory {
    let topLevelFunctions: [RustTopLevelFunction]
    let typeMembers: [RustTypeMember]
    let protocolRequirements: [RustProtocolRequirement]
}

private struct RustTopLevelFunction {
    let rustFile: String
    let rustName: String
    let swiftName: String
}

private struct RustTypeMember {
    let rustFile: String
    let typeName: String
    let rustName: String
    let swiftName: String
    let isInstanceMethod: Bool
    let firstParameterLabel: String?
}

private struct RustProtocolRequirement {
    let rustFile: String
    let protocolName: String
    let rustName: String
    let swiftName: String
}

private struct SwiftGeneratedSurface {
    let topLevelFunctions: Set<String>
    let publicTypeBlocks: [String: String]
}

final class RustSwiftSurfaceContractTests: DemoTestCase {
    func testGeneratedSwiftContainsAllRustTopLevelExports() throws {
        let rustExportInventory = try loadRustExportInventory()
        let swiftGeneratedSurface = try loadSwiftGeneratedSurface()

        let missingFunctions = rustExportInventory.topLevelFunctions.filter { rustFunction in
            !swiftGeneratedSurface.topLevelFunctions.contains(rustFunction.swiftName)
        }

        XCTAssertTrue(missingFunctions.isEmpty, formatMissingTopLevelFunctions(missingFunctions))
    }

    func testGeneratedSwiftContainsAllRustTypeMembers() throws {
        let rustExportInventory = try loadRustExportInventory()
        let swiftGeneratedSurface = try loadSwiftGeneratedSurface()

        let missingMembers = rustExportInventory.typeMembers.filter { rustMember in
            !swiftGeneratedSurface.contains(typeMember: rustMember)
        }

        XCTAssertTrue(missingMembers.isEmpty, formatMissingTypeMembers(missingMembers))
    }

    func testGeneratedSwiftContainsAllRustProtocolRequirements() throws {
        let rustExportInventory = try loadRustExportInventory()
        let swiftGeneratedSurface = try loadSwiftGeneratedSurface()

        let missingRequirements = rustExportInventory.protocolRequirements.filter { rustRequirement in
            !swiftGeneratedSurface.contains(protocolRequirement: rustRequirement)
        }

        XCTAssertTrue(missingRequirements.isEmpty, formatMissingProtocolRequirements(missingRequirements))
    }

    func testMatchingSwiftTestsReferenceAllRustTopLevelExports() throws {
        let rustExportInventory = try loadRustExportInventory()
        let swiftTestSources = try loadSwiftTestSources()

        let missingCoverage = rustExportInventory.topLevelFunctions.filter { rustFunction in
            let swiftTestFile = tryCoverageFile(for: rustFunction.rustFile)
            guard let swiftTestSource = swiftTestSources[swiftTestFile] else {
                return true
            }
            return !swiftTestSource.contains("\(rustFunction.swiftName)(")
        }

        XCTAssertTrue(missingCoverage.isEmpty, formatMissingTopLevelCoverage(missingCoverage))
    }

    func testMatchingSwiftTestsReferenceAllRustTypeMembers() throws {
        let rustExportInventory = try loadRustExportInventory()
        let swiftGeneratedSurface = try loadSwiftGeneratedSurface()
        let swiftTestSources = try loadSwiftTestSources()

        let missingCoverage = rustExportInventory.typeMembers.filter { rustMember in
            if typeMemberCoverageGaps.contains(typeMemberCoverageKey(rustMember)) {
                return false
            }
            let swiftTestFile = tryCoverageFile(for: rustMember.rustFile)
            guard let swiftTestSource = swiftTestSources[swiftTestFile] else {
                return true
            }
            let usageTokens = swiftGeneratedSurface.usageTokens(for: rustMember)
            return !usageTokens.contains { usageToken in
                swiftTestSource.contains(usageToken)
            }
        }

        XCTAssertTrue(missingCoverage.isEmpty, formatMissingTypeCoverage(missingCoverage))
    }

    func testMatchingSwiftTestsReferenceAllRustProtocolRequirements() throws {
        let rustExportInventory = try loadRustExportInventory()
        let swiftTestSources = try loadSwiftTestSources()

        let missingCoverage = rustExportInventory.protocolRequirements.filter { rustRequirement in
            let swiftTestFile = tryCoverageFile(for: rustRequirement.rustFile)
            guard let swiftTestSource = swiftTestSources[swiftTestFile] else {
                return true
            }
            return !swiftTestSource.contains("\(rustRequirement.swiftName)(")
        }

        XCTAssertTrue(missingCoverage.isEmpty, formatMissingProtocolCoverage(missingCoverage))
    }

    private func loadRustExportInventory() throws -> RustExportInventory {
        let rustSourceRoot = try repositoryRootURL()
            .appendingPathComponent("examples/demo/src")

        let rustFiles = try FileManager.default
            .subpathsOfDirectory(atPath: rustSourceRoot.path)
            .filter { $0.hasSuffix(".rs") && !featureScopedRustFiles.contains($0) }
            .sorted()

        let parsedInventories = try rustFiles.map { relativePath in
            try parseRustExports(
                source: String(contentsOf: rustSourceRoot.appendingPathComponent(relativePath)),
                rustFile: relativePath
            )
        }

        return RustExportInventory(
            topLevelFunctions: parsedInventories.flatMap(\.topLevelFunctions),
            typeMembers: parsedInventories.flatMap(\.typeMembers),
            protocolRequirements: parsedInventories.flatMap(\.protocolRequirements)
        )
    }

    private func loadSwiftGeneratedSurface() throws -> SwiftGeneratedSurface {
        let generatedSwiftURL = try repositoryRootURL()
            .appendingPathComponent("examples/platforms/apple/Sources/Demo/BoltFFI/DemoBoltFFI.swift")
        let generatedSwiftSource = try String(contentsOf: generatedSwiftURL)
        let sourceLines = generatedSwiftSource.components(separatedBy: .newlines)

        var publicTypeBlocks: [String: String] = [:]
        var lineIndex = 0

        while lineIndex < sourceLines.count {
            let trimmedLine = sourceLines[lineIndex].trimmingCharacters(in: .whitespaces)

            if let typeName = match(trimmedLine, pattern: #"^public (?:final class|class|struct|enum|protocol) ([A-Za-z0-9_]+)"#)?.first {
                let (blockLines, nextLineIndex) = captureBlock(from: sourceLines, startIndex: lineIndex)
                publicTypeBlocks[typeName, default: ""].append(blockLines.joined(separator: "\n"))
                lineIndex = nextLineIndex
                continue
            }

            if let typeName = match(trimmedLine, pattern: #"^extension ([A-Za-z0-9_]+)\s*\{"#)?.first {
                let (blockLines, nextLineIndex) = captureBlock(from: sourceLines, startIndex: lineIndex)
                publicTypeBlocks[typeName, default: ""].append(blockLines.joined(separator: "\n"))
                lineIndex = nextLineIndex
                continue
            }

            lineIndex += 1
        }

        let topLevelFunctions = Set(
            generatedSwiftSource
                .captures(pattern: #"(?m)^public func ([A-Za-z0-9_]+)\("#)
                .compactMap { captureGroups in
                    captureGroups.first
                }
        )

        return SwiftGeneratedSurface(
            topLevelFunctions: topLevelFunctions,
            publicTypeBlocks: publicTypeBlocks
        )
    }

    private func loadSwiftTestSources() throws -> [String: String] {
        let testsRoot = try repositoryRootURL()
            .appendingPathComponent("examples/platforms/apple/Tests/DemoConsumerTests")
        let testFiles = try FileManager.default
            .subpathsOfDirectory(atPath: testsRoot.path)
            .filter { $0.hasSuffix(".swift") && !$0.hasSuffix("DemoTestSupport.swift") && !$0.hasSuffix("RustSwiftSurfaceContractTests.swift") }
            .sorted()

        return try Dictionary(uniqueKeysWithValues: testFiles.map { relativePath in
            let contents = try String(contentsOf: testsRoot.appendingPathComponent(relativePath))
            return (relativePath, contents)
        })
    }

    private func parseRustExports(source: String, rustFile: String) throws -> RustExportInventory {
        let sourceLines = source.components(separatedBy: .newlines)
        var topLevelFunctions: [RustTopLevelFunction] = []
        var typeMembers: [RustTypeMember] = []
        var protocolRequirements: [RustProtocolRequirement] = []
        var lineIndex = 0

        while lineIndex < sourceLines.count {
            let trimmedLine = sourceLines[lineIndex].trimmingCharacters(in: .whitespaces)

            if trimmedLine.hasPrefix("#[export") {
                let itemIndex = nextItemLineIndex(in: sourceLines, startingAfter: lineIndex)
                guard itemIndex < sourceLines.count else {
                    break
                }

                let itemLine = sourceLines[itemIndex].trimmingCharacters(in: .whitespaces)

                if let functionName = match(itemLine, pattern: #"^pub(?:\s+async)?\s+fn\s+([A-Za-z0-9_]+)\s*\("#)?.first {
                    topLevelFunctions.append(
                        RustTopLevelFunction(
                            rustFile: rustFile,
                            rustName: functionName,
                            swiftName: functionName.lowerCamelCasedFromSnakeCase()
                        )
                    )
                    lineIndex = itemIndex + 1
                    continue
                }

                if let typeName = match(itemLine, pattern: #"^impl\s+([A-Za-z0-9_]+)\s*\{"#)?.first {
                    let (blockLines, nextLineIndex) = captureBlock(from: sourceLines, startIndex: itemIndex)
                    typeMembers.append(contentsOf: parseRustImplMembers(blockLines, rustFile: rustFile, typeName: typeName))
                    lineIndex = nextLineIndex
                    continue
                }

                if let protocolName = match(itemLine, pattern: #"^pub\s+trait\s+([A-Za-z0-9_]+)"#)?.first {
                    let (blockLines, nextLineIndex) = captureBlock(from: sourceLines, startIndex: itemIndex)
                    protocolRequirements.append(contentsOf: parseRustTraitMethods(blockLines, rustFile: rustFile, protocolName: protocolName))
                    lineIndex = nextLineIndex
                    continue
                }
            }

            if trimmedLine == "#[data(impl)]" {
                let itemIndex = nextItemLineIndex(in: sourceLines, startingAfter: lineIndex)
                guard itemIndex < sourceLines.count else {
                    break
                }

                let itemLine = sourceLines[itemIndex].trimmingCharacters(in: .whitespaces)

                if let typeName = match(itemLine, pattern: #"^impl\s+([A-Za-z0-9_]+)\s*\{"#)?.first {
                    let (blockLines, nextLineIndex) = captureBlock(from: sourceLines, startIndex: itemIndex)
                    typeMembers.append(contentsOf: parseRustImplMembers(blockLines, rustFile: rustFile, typeName: typeName))
                    lineIndex = nextLineIndex
                    continue
                }
            }

            lineIndex += 1
        }

        return RustExportInventory(
            topLevelFunctions: topLevelFunctions,
            typeMembers: typeMembers,
            protocolRequirements: protocolRequirements
        )
    }

    private func parseRustImplMembers(_ blockLines: [String], rustFile: String, typeName: String) -> [RustTypeMember] {
        var members: [RustTypeMember] = []
        var lineIndex = 0

        while lineIndex < blockLines.count {
            let signatureStart = blockLines[lineIndex].trimmingCharacters(in: .whitespaces)

            if let rustName = match(signatureStart, pattern: #"^pub(?:\s+async)?\s+fn\s+([A-Za-z0-9_]+)\s*\("#)?.first {
                let signature = captureSignature(from: blockLines, startIndex: lineIndex)
                let isInstanceMethod = signature.contains("&self") || signature.contains("&mut self")

                members.append(
                    RustTypeMember(
                        rustFile: rustFile,
                        typeName: typeName,
                        rustName: rustName,
                        swiftName: rustName.lowerCamelCasedFromSnakeCase(),
                        isInstanceMethod: isInstanceMethod
                        ,
                        firstParameterLabel: firstNonReceiverParameterLabel(in: signature)
                    )
                )
            }

            lineIndex += 1
        }

        return members
    }

    private func parseRustTraitMethods(_ blockLines: [String], rustFile: String, protocolName: String) -> [RustProtocolRequirement] {
        var requirements: [RustProtocolRequirement] = []
        var lineIndex = 0

        while lineIndex < blockLines.count {
            let signatureStart = blockLines[lineIndex].trimmingCharacters(in: .whitespaces)

            if let rustName = match(signatureStart, pattern: #"^(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*\("#)?.first {
                requirements.append(
                    RustProtocolRequirement(
                        rustFile: rustFile,
                        protocolName: protocolName,
                        rustName: rustName,
                        swiftName: rustName.lowerCamelCasedFromSnakeCase()
                    )
                )
            }

            lineIndex += 1
        }

        return requirements
    }

    private func firstNonReceiverParameterLabel(in signature: String) -> String? {
        guard let parameterList = match(signature, pattern: #"\((.*)\)"#)?.first else {
            return nil
        }

        return parameterList
            .captures(pattern: #"([A-Za-z0-9_]+)\s*:"#)
            .compactMap { $0.first }
            .first(where: { $0 != "self" })
    }

    private func nextItemLineIndex(in lines: [String], startingAfter lineIndex: Int) -> Int {
        var nextLineIndex = lineIndex + 1

        while nextLineIndex < lines.count {
            let trimmedLine = lines[nextLineIndex].trimmingCharacters(in: .whitespaces)
            if trimmedLine.isEmpty || trimmedLine.hasPrefix("#[") {
                nextLineIndex += 1
                continue
            }
            break
        }

        return nextLineIndex
    }

    private func captureBlock(from lines: [String], startIndex: Int) -> ([String], Int) {
        var blockLines: [String] = []
        var braceBalance = 0
        var lineIndex = startIndex

        repeat {
            let line = lines[lineIndex]
            blockLines.append(line)
            braceBalance += line.filter { $0 == "{" }.count
            braceBalance -= line.filter { $0 == "}" }.count
            lineIndex += 1
        } while lineIndex < lines.count && braceBalance > 0

        return (blockLines, lineIndex)
    }

    private func captureSignature(from blockLines: [String], startIndex: Int) -> String {
        var signature = blockLines[startIndex].trimmingCharacters(in: .whitespaces)
        var lineIndex = startIndex + 1

        while lineIndex < blockLines.count && !signature.contains("{") && !signature.contains(";") {
            signature += " " + blockLines[lineIndex].trimmingCharacters(in: .whitespaces)
            lineIndex += 1
        }

        return signature
    }

    private func formatMissingTopLevelFunctions(_ missingFunctions: [RustTopLevelFunction]) -> String {
        missingFunctions
            .map { missingFunction in
                "\(missingFunction.rustFile): missing Swift top-level function \(missingFunction.swiftName) for Rust export \(missingFunction.rustName)"
            }
            .joined(separator: "\n")
    }

    private func formatMissingTypeMembers(_ missingMembers: [RustTypeMember]) -> String {
        missingMembers
            .map { missingMember in
                "\(missingMember.rustFile): missing Swift member \(missingMember.typeName).\(missingMember.swiftName) for Rust member \(missingMember.rustName)"
            }
            .joined(separator: "\n")
    }

    private func formatMissingProtocolRequirements(_ missingRequirements: [RustProtocolRequirement]) -> String {
        missingRequirements
            .map { missingRequirement in
                "\(missingRequirement.rustFile): missing Swift protocol requirement \(missingRequirement.protocolName).\(missingRequirement.swiftName) for Rust requirement \(missingRequirement.rustName)"
            }
            .joined(separator: "\n")
    }

    private func formatMissingTopLevelCoverage(_ missingFunctions: [RustTopLevelFunction]) -> String {
        missingFunctions
            .map { missingFunction in
                "\(missingFunction.rustFile): matching Swift tests do not reference \(missingFunction.swiftName)"
            }
            .joined(separator: "\n")
    }

    private func formatMissingTypeCoverage(_ missingMembers: [RustTypeMember]) -> String {
        missingMembers
            .map { missingMember in
                "\(missingMember.rustFile): matching Swift tests do not reference \(missingMember.typeName).\(missingMember.swiftName)"
            }
            .joined(separator: "\n")
    }

    private func formatMissingProtocolCoverage(_ missingRequirements: [RustProtocolRequirement]) -> String {
        missingRequirements
            .map { missingRequirement in
                "\(missingRequirement.rustFile): matching Swift tests do not reference \(missingRequirement.protocolName).\(missingRequirement.swiftName)"
            }
            .joined(separator: "\n")
    }

    private func tryCoverageFile(for rustFile: String) -> String {
        rustToSwiftCoverageFile[rustFile] ?? "__missing__"
    }

    private func repositoryRootURL() throws -> URL {
        var currentURL = URL(fileURLWithPath: #filePath)

        while currentURL.path != "/" {
            let cargoManifestURL = currentURL.appendingPathComponent("Cargo.toml")
            let demoConfigURL = currentURL.appendingPathComponent("examples/demo/boltffi.toml")
            let applePackageURL = currentURL.appendingPathComponent("examples/platforms/apple/Package.swift")
            if FileManager.default.fileExists(atPath: cargoManifestURL.path)
                && FileManager.default.fileExists(atPath: demoConfigURL.path)
                && FileManager.default.fileExists(atPath: applePackageURL.path)
            {
                return currentURL
            }
            currentURL.deleteLastPathComponent()
        }

        throw NSError(domain: "RustSwiftSurfaceContractTests", code: 1, userInfo: [NSLocalizedDescriptionKey: "could not locate repository root"])
    }

    private func match(_ text: String, pattern: String) -> [String]? {
        guard let regularExpression = try? NSRegularExpression(pattern: pattern) else {
            return nil
        }

        let fullRange = NSRange(text.startIndex..., in: text)
        guard let match = regularExpression.firstMatch(in: text, range: fullRange) else {
            return nil
        }

        return (1..<match.numberOfRanges).compactMap { captureIndex in
            let captureRange = match.range(at: captureIndex)
            guard let range = Range(captureRange, in: text) else {
                return nil
            }
            return String(text[range])
        }
    }
}

private extension SwiftGeneratedSurface {
    func contains(typeMember rustTypeMember: RustTypeMember) -> Bool {
        guard let typeBlock = publicTypeBlocks[rustTypeMember.typeName] else {
            return false
        }

        return expectedPatterns(for: rustTypeMember).contains { expectedPattern in
            typeBlock.matches(pattern: expectedPattern)
        }
    }

    func contains(protocolRequirement rustProtocolRequirement: RustProtocolRequirement) -> Bool {
        guard let protocolBlock = publicTypeBlocks[rustProtocolRequirement.protocolName] else {
            return false
        }

        return protocolBlock.matches(
            pattern: "\\bfunc\\s+\(rustProtocolRequirement.swiftName)\\s*\\("
        )
    }

    func usageTokens(for rustTypeMember: RustTypeMember) -> [String] {
        guard let typeBlock = publicTypeBlocks[rustTypeMember.typeName] else {
            return []
        }

        if typeBlock.contains("public static func \(rustTypeMember.swiftName)(") {
            return ["\(rustTypeMember.typeName).\(rustTypeMember.swiftName)("]
        }

        if typeBlock.contains("public mutating func \(rustTypeMember.swiftName)(") || typeBlock.contains("public func \(rustTypeMember.swiftName)(") {
            return [".\(rustTypeMember.swiftName)("]
        }

        if rustTypeMember.rustName == "new" {
            if let firstParameterLabel = rustTypeMember.firstParameterLabel {
                return ["\(rustTypeMember.typeName)(\(firstParameterLabel):"]
            }
            return ["\(rustTypeMember.typeName)()"]
        }

        return [
            "\(rustTypeMember.typeName)(\(rustTypeMember.swiftName):",
            "\(rustTypeMember.typeName)?(\(rustTypeMember.swiftName):",
            "\(rustTypeMember.typeName).\(rustTypeMember.swiftName)("
        ]
    }

    private func expectedPatterns(for rustTypeMember: RustTypeMember) -> [String] {
        if rustTypeMember.rustName == "new" {
            return [
                "\\bpublic\\s+init\\s*\\(",
                "\\bpublic\\s+convenience\\s+init\\s*\\(",
                "\\bpublic\\s+static\\s+func\\s+new\\s*\\("
            ]
        }

        if rustTypeMember.isInstanceMethod {
            return [
                "\\bpublic\\s+func\\s+\(rustTypeMember.swiftName)\\s*\\(",
                "\\bpublic\\s+mutating\\s+func\\s+\(rustTypeMember.swiftName)\\s*\\("
            ]
        }

        return [
            "\\bpublic\\s+static\\s+func\\s+\(rustTypeMember.swiftName)\\s*\\(",
            "\\bpublic\\s+func\\s+\(rustTypeMember.swiftName)\\s*\\(",
            "\\bpublic\\s+init\\??\\s*\\(\\s*\(rustTypeMember.swiftName)\\b",
            "\\bpublic\\s+convenience\\s+init\\??\\s*\\(\\s*\(rustTypeMember.swiftName)\\b"
        ]
    }
}

// These members are generated for Swift, but the Swift demo tests do not
// exercise the C# regression cases yet. The demo metadata tracks them as
// coverage gaps.
private let typeMemberCoverageGaps: Set<String> = [
    "enums/data_enum.rs::Shape::maybeCircle",
    "records/default_values.rs::ServiceConfig::tryWithRetries",
    "records/default_values.rs::ServiceConfig::maybeWithRetries"
]

private let featureScopedRustFiles: Set<String> = [
    "callbacks/csharp_closures.rs",
    "classes/async_factory.rs"
]

private func typeMemberCoverageKey(_ rustTypeMember: RustTypeMember) -> String {
    "\(rustTypeMember.rustFile)::\(rustTypeMember.typeName)::\(rustTypeMember.swiftName)"
}

private let rustToSwiftCoverageFile: [String: String] = [
    "async_fns/mod.rs": "async_fns/AsyncFnsTests.swift",
    "builtins/mod.rs": "builtins/BuiltinsTests.swift",
    "bytes/mod.rs": "bytes/BytesTests.swift",
    "callbacks/async_traits.rs": "callbacks/AsyncTraitsTests.swift",
    "callbacks/closures.rs": "callbacks/ClosuresTests.swift",
    "callbacks/sync_traits.rs": "callbacks/SyncTraitsTests.swift",
    "classes/async_methods.rs": "classes/AsyncMethodsTests.swift",
    "classes/borrowed.rs": "classes/BorrowedTests.swift",
    "classes/constructor_matrix.rs": "classes/ConstructorCoverageMatrixTests.swift",
    "classes/constructors.rs": "classes/ConstructorsTests.swift",
    "classes/methods.rs": "classes/MethodsTests.swift",
    "classes/static_methods.rs": "classes/StaticMethodsTests.swift",
    "classes/streams.rs": "classes/StreamsTests.swift",
    "classes/thread_safe.rs": "classes/ThreadSafeTests.swift",
    "classes/unsafe_single_threaded.rs": "classes/UnsafeSingleThreadedTests.swift",
    "collections/mod.rs": "collections/CollectionsTests.swift",
    "custom_types/mod.rs": "custom_types/CustomTypesTests.swift",
    "enums/c_style.rs": "enums/CStyleEnumsTests.swift",
    "enums/complex_variants.rs": "enums/ComplexVariantsEnumsTests.swift",
    "enums/data_enum.rs": "enums/DataEnumTests.swift",
    "enums/repr_int.rs": "enums/ReprIntEnumsTests.swift",
    "multicrate/mod.rs": "multicrate/MultiCrateTests.swift",
    "options/complex.rs": "options/ComplexOptionsTests.swift",
    "options/primitives.rs": "options/PrimitivesOptionsTests.swift",
    "primitives/scalars.rs": "primitives/ScalarsTests.swift",
    "primitives/strings.rs": "primitives/StringsTests.swift",
    "primitives/vecs.rs": "primitives/VecsTests.swift",
    "records/blittable.rs": "records/BlittableRecordsTests.swift",
    "records/default_values.rs": "records/DefaultValuesRecordsTests.swift",
    "records/mixed.rs": "records/MixedRecordsTests.swift",
    "records/nested.rs": "records/NestedRecordsTests.swift",
    "records/with_collections.rs": "records/WithCollectionsRecordsTests.swift",
    "records/with_enums.rs": "records/WithEnumsRecordsTests.swift",
    "records/with_options.rs": "records/WithOptionsRecordsTests.swift",
    "records/with_strings.rs": "records/WithStringsRecordsTests.swift",
    "results/async_results.rs": "results/AsyncResultsTests.swift",
    "results/basic.rs": "results/BasicResultsTests.swift",
    "results/error_enums.rs": "results/ErrorEnumsResultsTests.swift",
    "results/nested_results.rs": "results/NestedResultsTests.swift"
]

private extension String {
    func lowerCamelCasedFromSnakeCase() -> String {
        let components = split(separator: "_")
        guard let firstComponent = components.first else {
            return self
        }

        let remainingComponents = components.dropFirst().map { component in
            component.prefix(1).uppercased() + component.dropFirst()
        }

        return String(firstComponent) + remainingComponents.joined()
    }

    func captures(pattern: String) -> [[String]] {
        guard let regularExpression = try? NSRegularExpression(pattern: pattern) else {
            return []
        }

        let fullRange = NSRange(startIndex..., in: self)
        return regularExpression.matches(in: self, range: fullRange).map { match in
            (1..<match.numberOfRanges).compactMap { captureIndex in
                let captureRange = match.range(at: captureIndex)
                guard let range = Range(captureRange, in: self) else {
                    return nil
                }
                return String(self[range])
            }
        }
    }

    func matches(pattern: String) -> Bool {
        guard let regularExpression = try? NSRegularExpression(pattern: pattern) else {
            return false
        }

        let fullRange = NSRange(startIndex..., in: self)
        return regularExpression.firstMatch(in: self, range: fullRange) != nil
    }
}
