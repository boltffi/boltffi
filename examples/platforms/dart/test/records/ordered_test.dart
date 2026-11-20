import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('ordered records compare by their Rust Ord', () {
    final low = Rank(tier: 1, score: 9);
    final high = Rank(tier: 2, score: 0);
    expect(low.compareTo(high), -1);
    expect(high.compareTo(low), 1);
    expect(low.compareTo(Rank(tier: 1, score: 9)), 0);
    expect(low < high && high >= Rank(tier: 2, score: 0), isTrue);
    expect(
      [high, Rank(tier: 1, score: 10), low]..sort(),
      [low, Rank(tier: 1, score: 10), high],
      reason: "case:records.ordered.rank.should_compare_by_rust_ord",
    );

    final alpha = SemanticVersion(major: 1, minor: 2, patch: 3, pre: 'alpha');
    final beta = SemanticVersion(major: 1, minor: 2, patch: 3, pre: 'beta');
    final release = SemanticVersion(major: 1, minor: 2, patch: 3, pre: null);
    final nextPatch = SemanticVersion(major: 1, minor: 2, patch: 4, pre: 'alpha');
    expect(alpha < beta, isTrue);
    expect(beta < release, isTrue);
    expect(release < nextPatch, isTrue);
    expect(
      release.compareTo(SemanticVersion(major: 1, minor: 2, patch: 3, pre: null)),
      0,
    );
    expect(
      [nextPatch, release, beta, alpha]..sort(),
      [alpha, beta, release, nextPatch],
      reason: "case:records.ordered.semantic_version.should_compare_by_rust_ord",
    );
  });
}
