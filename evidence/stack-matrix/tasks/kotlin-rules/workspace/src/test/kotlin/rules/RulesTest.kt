package rules

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class RulesTest {
    private fun eval(source: String, vararg context: Pair<String, Any?>) = Rules.evaluate(Rules.parse(source), mapOf(*context))

    private fun parseError(source: String): Int = assertFailsWith<RuleError>(source) { Rules.parse(source) }.position

    @Test
    fun theExampleFromTheReadme() {
        val rule = """country in ["IT", "FR"] and (age >= 18 or plan == "pro")"""
        assertTrue(eval(rule, "country" to "IT", "age" to 17, "plan" to "pro"))
        assertTrue(eval(rule, "country" to "FR", "age" to 30, "plan" to "free"))
        assertFalse(eval(rule, "country" to "US", "age" to 30, "plan" to "pro"))
        assertFalse(eval(rule, "country" to "IT", "age" to 17, "plan" to "free"))
    }

    @Test
    fun precedenceOrAndNot() {
        assertTrue(eval("a or b and c", "a" to true, "b" to false, "c" to false))
        assertFalse(eval("(a or b) and c", "a" to true, "b" to false, "c" to false))
        assertTrue(eval("not a and b", "a" to false, "b" to true))
        assertFalse(eval("not (a and b)", "a" to true, "b" to true))
        assertTrue(eval("country not in [\"US\", \"CA\"]", "country" to "IT"))
    }

    @Test
    fun numbersStringsAndBooleans() {
        assertTrue(eval("score > 4.5", "score" to 5))
        assertTrue(eval("age == 18.0", "age" to 18))
        assertTrue(eval("level <= -2", "level" to -3))
        assertTrue(eval("name == \"Ann \\\"A\\\"\"", "name" to "Ann \"A\""))
        assertTrue(eval("tier < \"gold\"", "tier" to "bronze"))
        assertTrue(eval("beta == true and enabled != false", "beta" to true, "enabled" to true))
        assertTrue(eval("user.age >= 21", "user.age" to 21))
        assertFalse(eval("x in []", "x" to 1))
        assertTrue(eval("x in [1, \"1\", 2.5]", "x" to 2.5))
    }

    @Test
    fun missingNamesMakeComparisonsFalse() {
        assertFalse(eval("age > 18"))
        assertFalse(eval("age != 18"))
        assertFalse(eval("age in [1, 2]"))
        assertTrue(eval("not (age > 18)"))
        assertFalse(eval("age == 1", "age" to null))
    }

    @Test
    fun typeErrorsAreRuleErrors() {
        val mismatch = assertFailsWith<RuleError> { eval("x and name > 3", "x" to true, "name" to "n") }
        assertEquals(6, mismatch.position)
        assertFailsWith<RuleError> { eval("flag < true", "flag" to false) }
        assertEquals(5, assertFailsWith<RuleError> { eval("a or b", "a" to false, "b" to 3) }.position)
        assertEquals(0, assertFailsWith<RuleError> { eval("missing") }.position)
    }

    @Test
    fun syntaxErrorsCarryPositions() {
        assertEquals(7, parseError("age >= "))
        assertEquals(5, parseError("age >> 3"))
        assertEquals(8, parseError("name == \"abc"))
        assertEquals(7, parseError("(a == 1"))
        assertEquals(7, parseError("a == 1 b"))
        assertEquals(2, parseError("a # b"))
        assertEquals(5, parseError("a in 3"))
        assertEquals(0, parseError(""))
    }
}
