package rules

class RuleError(message: String, val position: Int) : Exception(message)

sealed class Expr {
    abstract val position: Int
}

data class Or(val left: Expr, val right: Expr, override val position: Int) : Expr()
data class And(val left: Expr, val right: Expr, override val position: Int) : Expr()
data class Not(val operand: Expr, override val position: Int) : Expr()
data class Compare(val op: String, val left: Operand, val right: Operand, override val position: Int) : Expr()
data class InList(val operand: Operand, val items: List<Any>, val negated: Boolean, override val position: Int) : Expr()
data class Bare(val operand: Operand, override val position: Int) : Expr()

sealed class Operand {
    abstract val position: Int
}

data class Name(val name: String, override val position: Int) : Operand()
data class Literal(val value: Any, override val position: Int) : Operand()

private enum class Kind { NAME, NUMBER, STRING, TRUE, FALSE, AND, OR, NOT, IN, OP, LPAREN, RPAREN, LBRACKET, RBRACKET, COMMA, END }

private data class Token(val kind: Kind, val text: String, val value: Any?, val position: Int)

private val KEYWORDS = mapOf("and" to Kind.AND, "or" to Kind.OR, "not" to Kind.NOT, "in" to Kind.IN, "true" to Kind.TRUE, "false" to Kind.FALSE)

private fun lex(source: String): List<Token> {
    val tokens = mutableListOf<Token>()
    var i = 0
    while (i < source.length) {
        val c = source[i]
        when {
            c.isWhitespace() -> i++
            c == '(' -> { tokens += Token(Kind.LPAREN, "(", null, i); i++ }
            c == ')' -> { tokens += Token(Kind.RPAREN, ")", null, i); i++ }
            c == '[' -> { tokens += Token(Kind.LBRACKET, "[", null, i); i++ }
            c == ']' -> { tokens += Token(Kind.RBRACKET, "]", null, i); i++ }
            c == ',' -> { tokens += Token(Kind.COMMA, ",", null, i); i++ }
            c == '=' || c == '!' -> {
                if (i + 1 < source.length && source[i + 1] == '=') {
                    tokens += Token(Kind.OP, "$c=", null, i); i += 2
                } else throw RuleError("unexpected character '$c'", i)
            }
            c == '<' || c == '>' -> {
                if (i + 1 < source.length && source[i + 1] == '=') {
                    tokens += Token(Kind.OP, "$c=", null, i); i += 2
                } else {
                    tokens += Token(Kind.OP, "$c", null, i); i++
                }
            }
            c == '"' -> {
                val start = i
                val text = StringBuilder()
                i++
                var closed = false
                while (i < source.length) {
                    val d = source[i]
                    if (d == '\\' && i + 1 < source.length) {
                        text.append(source[i + 1]); i += 2
                    } else if (d == '"') {
                        closed = true; i++; break
                    } else {
                        text.append(d); i++
                    }
                }
                if (!closed) throw RuleError("unterminated string", start)
                tokens += Token(Kind.STRING, source.substring(start, i), text.toString(), start)
            }
            c.isDigit() || (c == '-' && i + 1 < source.length && source[i + 1].isDigit()) -> {
                val start = i
                i++
                while (i < source.length && source[i].isDigit()) i++
                var decimal = false
                if (i + 1 < source.length && source[i] == '.' && source[i + 1].isDigit()) {
                    decimal = true
                    i++
                    while (i < source.length && source[i].isDigit()) i++
                }
                val text = source.substring(start, i)
                val value: Any = if (decimal) text.toDouble() else text.toLongOrNull() ?: text.toDouble()
                tokens += Token(Kind.NUMBER, text, value, start)
            }
            c.isLetter() || c == '_' -> {
                val start = i
                while (i < source.length && (source[i].isLetterOrDigit() || source[i] == '_' || source[i] == '.')) i++
                val text = source.substring(start, i)
                val kind = KEYWORDS[text] ?: Kind.NAME
                tokens += Token(kind, text, if (kind == Kind.TRUE) true else if (kind == Kind.FALSE) false else null, start)
            }
            else -> throw RuleError("unexpected character '$c'", i)
        }
    }
    tokens += Token(Kind.END, "", null, source.length)
    return tokens
}

private class Parser(private val tokens: List<Token>) {
    private var at = 0
    private val next get() = tokens[at]

    private fun fail(token: Token): Nothing =
        throw RuleError(if (token.kind == Kind.END) "unexpected end of rule" else "unexpected '${token.text}'", token.position)

    fun rule(): Expr {
        val expr = or()
        if (next.kind != Kind.END) fail(next)
        return expr
    }

    private fun or(): Expr {
        var left = and()
        while (next.kind == Kind.OR) {
            at++
            left = Or(left, and(), left.position)
        }
        return left
    }

    private fun and(): Expr {
        var left = not()
        while (next.kind == Kind.AND) {
            at++
            left = And(left, not(), left.position)
        }
        return left
    }

    private fun not(): Expr {
        if (next.kind == Kind.NOT) {
            val position = next.position
            at++
            return Not(not(), position)
        }
        return comparison()
    }

    private fun comparison(): Expr {
        if (next.kind == Kind.LPAREN) {
            at++
            val inner = or()
            if (next.kind != Kind.RPAREN) fail(next)
            at++
            return inner
        }
        val left = operand()
        return when {
            next.kind == Kind.OP -> {
                val op = next.text
                at++
                Compare(op, left, operand(), left.position)
            }
            next.kind == Kind.IN -> { at++; InList(left, list(), false, left.position) }
            next.kind == Kind.NOT && tokens[at + 1].kind == Kind.IN -> { at += 2; InList(left, list(), true, left.position) }
            else -> Bare(left, left.position)
        }
    }

    private fun operand(): Operand {
        val token = next
        return when (token.kind) {
            Kind.NAME -> { at++; Name(token.text, token.position) }
            Kind.NUMBER, Kind.STRING, Kind.TRUE, Kind.FALSE -> { at++; Literal(token.value!!, token.position) }
            else -> fail(token)
        }
    }

    private fun list(): List<Any> {
        if (next.kind != Kind.LBRACKET) fail(next)
        at++
        val items = mutableListOf<Any>()
        if (next.kind == Kind.RBRACKET) {
            at++
            return items
        }
        while (true) {
            val token = next
            if (token.kind !in setOf(Kind.NUMBER, Kind.STRING, Kind.TRUE, Kind.FALSE)) fail(token)
            items += token.value!!
            at++
            when (next.kind) {
                Kind.COMMA -> at++
                Kind.RBRACKET -> { at++; return items }
                else -> fail(next)
            }
        }
    }
}

object Rules {
    fun parse(source: String): Expr = Parser(lex(source)).rule()

    fun evaluate(expr: Expr, context: Map<String, Any?>): Boolean = when (expr) {
        is Or -> evaluate(expr.left, context) || evaluate(expr.right, context)
        is And -> evaluate(expr.left, context) && evaluate(expr.right, context)
        is Not -> !evaluate(expr.operand, context)
        is Bare -> resolve(expr.operand, context) as? Boolean ?: throw RuleError("not a boolean", expr.position)
        is Compare -> {
            val left = resolve(expr.left, context)
            val right = resolve(expr.right, context)
            if (left == null || right == null) false else compare(expr.op, left, right, expr.position)
        }
        is InList -> {
            val value = resolve(expr.operand, context)
            if (value == null) false else expr.items.any { same(value, normalize(it)) } != expr.negated
        }
    }

    private fun normalize(value: Any?): Any? = when (value) {
        is Byte, is Short, is Int, is Long -> (value as Number).toLong()
        is Float, is Double -> (value as Number).toDouble()
        else -> value
    }

    private fun resolve(operand: Operand, context: Map<String, Any?>): Any? = when (operand) {
        is Name -> normalize(context[operand.name])
        is Literal -> normalize(operand.value)
    }

    private fun same(a: Any?, b: Any?): Boolean = when {
        a is Long && b is Long -> a == b
        a is Number && b is Number -> a.toDouble() == b.toDouble()
        else -> a == b
    }

    private fun compare(op: String, left: Any, right: Any, position: Int): Boolean {
        val order: Int = when {
            left is Long && right is Long -> left.compareTo(right)
            left is Number && right is Number -> left.toDouble().compareTo(right.toDouble())
            left is String && right is String -> left.compareTo(right)
            left is Boolean && right is Boolean -> {
                return when (op) {
                    "==" -> left == right
                    "!=" -> left != right
                    else -> throw RuleError("booleans cannot be ordered", position)
                }
            }
            else -> throw RuleError("cannot compare ${left::class.simpleName} with ${right::class.simpleName}", position)
        }
        return when (op) {
            "==" -> order == 0
            "!=" -> order != 0
            "<" -> order < 0
            "<=" -> order <= 0
            ">" -> order > 0
            else -> order >= 0
        }
    }
}
