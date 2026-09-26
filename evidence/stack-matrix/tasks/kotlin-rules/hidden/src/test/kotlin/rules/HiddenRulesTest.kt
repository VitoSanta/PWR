package rules

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class HiddenRulesTest {
    private fun eval(source: String, vararg context: Pair<String, Any?>) = Rules.evaluate(Rules.parse(source), mapOf(*context))

    @Test
    fun keywordsAreNotNames() {
        assertFailsWith<RuleError> { Rules.parse("and == 1") }
        assertTrue(eval("android == 1", "android" to 1))
        assertTrue(eval("order_2.in == 3", "order_2.in" to 3))
    }

    @Test
    fun deepNestingAndWhitespace() {
        assertTrue(eval("  ((( a ))) and\n\tnot(b)  ", "a" to true, "b" to false))
    }

    @Test
    fun longAndLeftToRightChains() {
        assertTrue(eval("a or b or c or d", "a" to false, "b" to false, "c" to false, "d" to true))
        assertFalse(eval("a and b and c", "a" to true, "b" to true, "c" to false))
    }

    @Test
    fun literalOnBothSidesAndLongs() {
        assertTrue(eval("3 < 4"))
        assertTrue(eval("n == 10000000000", "n" to 10_000_000_000L))
        assertTrue(eval("\"a\\\\b\" == s", "s" to "a\\b"))
    }

    @Test
    fun elementsOfAnotherTypeDoNotMatch() {
        assertFalse(eval("x in [\"1\", true]", "x" to 1))
        assertTrue(eval("flag in [false]", "flag" to false))
    }

    @Test
    fun moreSyntaxErrors() {
        assertEquals(4, assertFailsWith<RuleError> { Rules.parse("a ==") }.position)
        assertEquals(8, assertFailsWith<RuleError> { Rules.parse("x in [1,") }.position)
        assertEquals(1, assertFailsWith<RuleError> { Rules.parse("a) ") }.position)
    }
}
