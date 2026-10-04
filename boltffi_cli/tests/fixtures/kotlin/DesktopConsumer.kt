package com.boltffi.desktop

fun main() {
    Journey.Ledger.new(Long.MIN_VALUE).use { check(it.balance() == Long.MIN_VALUE) }
    Journey.Tally.new(ULong.MAX_VALUE).use { check(it.count() == ULong.MAX_VALUE) }
}
