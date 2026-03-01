#![cfg(not(miri))]

use boltffi_verify::{Reporter, VerificationResult, Verifier, ViolationKind};
use std::path::Path;

fn verify_swift(source: &str) -> VerificationResult {
    let mut verifier = Verifier::swift().expect("failed to create verifier");
    verifier
        .verify_source(Path::new("test.swift"), source)
        .expect("failed to verify")
}

#[test]
fn test_verify_generated_benchboltffi() {
    let swift_path = Path::new("../benchmarks/rust-boltffi/dist/BenchBoltFFI.swift");

    if !swift_path.exists() {
        eprintln!("Skipping test: BenchBoltFFI.swift not found (run `boltffi pack` first)");
        return;
    }

    let mut verifier = Verifier::swift().expect("failed to create verifier");
    let result = verifier.verify_file(swift_path).expect("failed to verify");

    let reporter = Reporter::human();
    eprintln!("{}", reporter.report(&result));

    eprintln!(
        "Verified {} functions",
        match &result {
            VerificationResult::Verified { unit_count, .. } => unit_count,
            VerificationResult::Failed { .. } => &0,
        }
    );
}

#[test]
fn test_verify_simple_generated_patterns() {
    let source = r#"
import Foundation

public struct FfiString {
    var ptr: UnsafePointer<UInt8>?
    var len: UInt
    var cap: UInt
}

public struct FfiStatus {
    var code: Int32
}

private func stringFromFfi(_ ffi: FfiString) -> String {
    guard let ptr = ffi.ptr else { return "" }
    return String(cString: ptr)
}

private func ensureOk(_ status: FfiStatus) {
    if status.code != 0 {
        fatalError("FFI error: \(status.code)")
    }
}

public func generateLocations(count: Int32) -> [Location] {
    let len = boltffi_generate_locations_len(count)
    let ptr = UnsafeMutablePointer<Location>.allocate(capacity: Int(len))
    defer { ptr.deallocate() }
    var written: UInt = 0
    let status = boltffi_generate_locations_copy_into(count, ptr, len, &written)
    ensureOk(status)
    return Array(UnsafeBufferPointer(start: ptr, count: Int(written)))
}

public func echoString(value: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
    return value.withCString { valuePtr in
        let status = boltffi_echo_string(UnsafeRawPointer(valuePtr).assumingMemoryBound(to: UInt8.self), UInt(value.utf8.count), &result)
        defer { boltffi_free_string(result) }
        ensureOk(status)
        return stringFromFfi(result)
    }
}
"#;

    let mut verifier = Verifier::swift().expect("failed to create verifier");
    let result = verifier
        .verify_source(std::path::Path::new("test.swift"), source)
        .expect("failed to verify");

    let reporter = Reporter::human();
    eprintln!("{}", reporter.report(&result));
}

#[test]
fn test_detects_memory_leak() {
    let source = r#"
public func leaksMemory() {
    let ptr = UnsafeMutablePointer<Int32>.allocate(capacity: 10)
    // No deallocate - this is a leak!
}
"#;
    let result = verify_swift(source);
    assert!(result.is_failed(), "Should detect memory leak");
    assert!(result.error_count() > 0);

    if let VerificationResult::Failed { violations, .. } = &result {
        assert!(
            violations
                .iter()
                .any(|v| matches!(v.kind, ViolationKind::MemoryLeak { .. }))
        );
    }
}

#[test]
fn test_detects_double_free() {
    let source = r#"
public func doublesFree() {
    let ptr = UnsafeMutablePointer<Int32>.allocate(capacity: 10)
    ptr.deallocate()
    ptr.deallocate()
}
"#;
    let result = verify_swift(source);
    assert!(result.is_failed(), "Should detect double free");

    if let VerificationResult::Failed { violations, .. } = &result {
        assert!(
            violations
                .iter()
                .any(|v| matches!(v.kind, ViolationKind::DoubleFree { .. }))
        );
    }
}

#[test]
fn test_detects_retain_leak() {
    let source = r#"
public func leaksRetain() {
    let obj = MyObject()
    let handle = Unmanaged.passRetained(obj).toOpaque()
    // No release - this is a retain leak!
}
"#;
    let result = verify_swift(source);
    assert!(result.is_failed(), "Should detect retain leak");

    if let VerificationResult::Failed { violations, .. } = &result {
        assert!(
            violations
                .iter()
                .any(|v| matches!(v.kind, ViolationKind::RetainLeak { .. }))
        );
    }
}

#[test]
fn test_detects_double_release() {
    let source = r#"
public func doublesRelease() {
    let obj = MyObject()
    let handle = Unmanaged.passRetained(obj).toOpaque()
    Unmanaged<MyObject>.fromOpaque(handle).release()
    Unmanaged<MyObject>.fromOpaque(handle).release()
}
"#;
    let result = verify_swift(source);
    assert!(result.is_failed(), "Should detect double release");

    if let VerificationResult::Failed { violations, .. } = &result {
        assert!(
            violations
                .iter()
                .any(|v| matches!(v.kind, ViolationKind::DoubleRelease { .. }))
        );
    }
}

#[test]
fn test_correct_code_passes() {
    let source = r#"
public func correctAlloc() {
    let ptr = UnsafeMutablePointer<Int32>.allocate(capacity: 10)
    defer { ptr.deallocate() }
}

public func correctRetain() {
    let obj = MyObject()
    let handle = Unmanaged.passRetained(obj).toOpaque()
    Unmanaged<MyObject>.fromOpaque(handle).release()
}
"#;
    let result = verify_swift(source);
    assert!(
        result.is_verified(),
        "Correct code should pass verification"
    );
}

#[test]
fn test_callback_bridge_pattern_no_false_positive() {
    let source = r#"
public enum DataProviderBridge {
    static func create(_ provider: DataProvider) -> UnsafeMutableRawPointer {
        let box = DataProviderBox(provider)
        return Unmanaged.passRetained(box).toOpaque()
    }
}
"#;
    let result = verify_swift(source);
    assert!(
        result.is_verified(),
        "Callback bridge pattern should not trigger false positive"
    );
}

#[test]
fn test_continuation_box_pattern_no_false_positive() {
    let source = r#"
final class ContinuationBox {
    let continuation: Continuation
    init(_ continuation: Continuation) { self.continuation = continuation }
}

func installContinuation(_ continuation: Continuation) -> Bool {
    let box = ContinuationBox(continuation)
    let raw = UInt64(UInt(bitPattern: Unmanaged.passRetained(box).toOpaque()))
    return true
}
"#;
    let result = verify_swift(source);
    assert!(
        result.is_verified(),
        "ContinuationBox pattern should not trigger false positive"
    );
}

#[test]
fn test_take_retained_value_pattern() {
    let source = r#"
func decideFinish(prior: UInt) -> Continuation {
    let box = Unmanaged<ContinuationBox>.fromOpaque(UnsafeRawPointer(bitPattern: prior)!).takeRetainedValue()
    return box.continuation
}
"#;
    let result = verify_swift(source);
    assert!(
        result.is_verified(),
        "takeRetainedValue pattern should pass"
    );
}

#[test]
fn test_async_rust_future_pattern() {
    let source = r#"
public final class RustFuture<T> {
    final class ContinuationBox {
        let continuation: Continuation
        init(_ continuation: Continuation) { self.continuation = continuation }
    }

    func installContinuation(_ continuation: Continuation) -> Bool {
        let box = ContinuationBox(continuation)
        let raw = UInt64(UInt(bitPattern: Unmanaged.passRetained(box).toOpaque()))
        return true
    }

    func decideFinish(prior: UInt) -> ContinuationBox {
        let box = Unmanaged<ContinuationBox>.fromOpaque(UnsafeRawPointer(bitPattern: prior)!).takeRetainedValue()
        return box
    }
}
"#;
    let result = verify_swift(source);
    assert!(
        result.is_verified(),
        "RustFuture async pattern should pass verification"
    );
}
