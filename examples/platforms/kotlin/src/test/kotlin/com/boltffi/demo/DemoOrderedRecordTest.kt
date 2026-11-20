package com.boltffi.demo

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class DemoOrderedRecordTest {
    @Test
    fun rankComparesByItsDerivedOrd() {
        demoCase("case:records.ordered.rank.should_compare_by_rust_ord")
        val low = Rank(1u, 9)
        val high = Rank(2u, 0)

        assertEquals(-1, low.compareTo(high))
        assertEquals(1, high.compareTo(low))
        assertEquals(0, low.compareTo(Rank(1u, 9)))
        assertTrue(low < high)
        assertTrue(high >= Rank(2u, 0))
        assertEquals(listOf(low, Rank(1u, 10), high), listOf(high, Rank(1u, 10), low).sorted())
    }

    @Test
    fun versionComparesByItsHandWrittenOrd() {
        demoCase("case:records.ordered.semantic_version.should_compare_by_rust_ord")
        val alpha = SemanticVersion(1u, 2u, 3u, "alpha")
        val beta = SemanticVersion(1u, 2u, 3u, "beta")
        val release = SemanticVersion(1u, 2u, 3u, null)
        val nextPatch = SemanticVersion(1u, 2u, 4u, "alpha")

        assertTrue(alpha < beta)
        assertTrue(beta < release)
        assertTrue(release < nextPatch)
        assertEquals(0, release.compareTo(SemanticVersion(1u, 2u, 3u, null)))
        assertEquals(listOf(alpha, beta, release, nextPatch), listOf(nextPatch, release, beta, alpha).sorted())
        assertEquals(release, listOf(beta, release, alpha).max())
    }
}
