package io.github.rayhungkao.pqtunnel

const val DEFAULT_RELAY_PORT = 51820

/**
 * Parses what the user typed as the relay address: `host`, `host:port`, `[ipv6]`, `[ipv6]:port`,
 * or a bare IPv6 address without a port. Returns null for anything else, including an empty host,
 * a colon with no port after it, or a port outside 1-65535.
 */
fun parseRelay(text: String): Pair<String, Int>? {
    val (host, port) = when {
        text.startsWith("[") -> {
            val close = text.indexOf(']')
            if (close < 0) return null
            val rest = text.substring(close + 1)
            val port = when {
                rest.isEmpty() -> DEFAULT_RELAY_PORT
                rest.startsWith(":") -> rest.drop(1).toIntOrNull() ?: return null
                else -> return null
            }
            text.substring(1, close) to port
        }
        // Two or more colons without brackets can only be an IPv6 address, so there is no port.
        text.count { it == ':' } > 1 -> text to DEFAULT_RELAY_PORT
        ':' in text -> text.substringBefore(':') to (text.substringAfter(':').toIntOrNull() ?: return null)
        else -> text to DEFAULT_RELAY_PORT
    }
    return if (host.isNotEmpty() && port in 1..65535) host to port else null
}
