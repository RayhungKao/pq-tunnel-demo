package io.github.rayhungkao.pqtunnel

const val DEFAULT_RELAY_PORT = 51820

/**
 * Parses what the user typed as the relay address: `host`, `host:port`, `[ipv6]`, `[ipv6]:port`,
 * or a bare IPv6 address without a port. Returns null for anything else, including an empty host,
 * a colon with no port after it, a port that is not plain digits or is outside 1-65535, and text
 * with two or more colons that is not an IPv6 address. Zone ids (`fe80::1%wlan0`) are not supported.
 */
fun parseRelay(text: String): Pair<String, Int>? {
    val (host, port) = when {
        text.startsWith("[") -> {
            val close = text.indexOf(']')
            if (close < 0) return null
            val inside = text.substring(1, close)
            // An embedded IPv4 address (::ffff:10.0.2.2) needs the brackets, so dots are allowed here.
            if (!isIpv6Literal(inside, allowDots = true)) return null
            val rest = text.substring(close + 1)
            val port = when {
                rest.isEmpty() -> DEFAULT_RELAY_PORT
                rest.startsWith(":") -> portOf(rest.drop(1)) ?: return null
                else -> return null
            }
            inside to port
        }
        // Two or more colons without brackets can only be an IPv6 address, so there is no port.
        text.count { it == ':' } > 1 -> if (isIpv6Literal(text, allowDots = false)) text to DEFAULT_RELAY_PORT else return null
        ':' in text -> text.substringBefore(':') to (portOf(text.substringAfter(':')) ?: return null)
        else -> text to DEFAULT_RELAY_PORT
    }
    return if (host.isNotEmpty() && port in 1..65535) host to port else null
}

/** Plain decimal digits only: `toIntOrNull` alone would also take "+80". */
private fun portOf(text: String): Int? =
    if (text.isNotEmpty() && text.length <= 5 && text.all { it in '0'..'9' }) text.toInt() else null

/** A rough IPv6 check: at least one colon, and only hex digits and colons (and dots if allowed). */
private fun isIpv6Literal(text: String, allowDots: Boolean): Boolean =
    ':' in text && text.all { it == ':' || it in '0'..'9' || it.lowercaseChar() in 'a'..'f' || (allowDots && it == '.') }
