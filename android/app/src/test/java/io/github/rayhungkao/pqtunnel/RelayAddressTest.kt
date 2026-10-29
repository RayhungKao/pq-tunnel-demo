package io.github.rayhungkao.pqtunnel

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RelayAddressTest {
    @Test
    fun hostAndPort() {
        assertEquals("10.0.2.2" to 51820, parseRelay("10.0.2.2:51820"))
        assertEquals("relay.example.com" to 4000, parseRelay("relay.example.com:4000"))
    }

    @Test
    fun hostAloneGetsTheDefaultPort() {
        assertEquals("10.0.2.2" to DEFAULT_RELAY_PORT, parseRelay("10.0.2.2"))
    }

    @Test
    fun ipv6InBracketsWithOrWithoutPort() {
        assertEquals("::1" to 4000, parseRelay("[::1]:4000"))
        assertEquals("fe80::1" to DEFAULT_RELAY_PORT, parseRelay("[fe80::1]"))
    }

    @Test
    fun bareIpv6HasNoPort() {
        assertEquals("fe80::1" to DEFAULT_RELAY_PORT, parseRelay("fe80::1"))
    }

    @Test
    fun ipv6WithAnEmbeddedIpv4AddressInBrackets() {
        assertEquals("::ffff:10.0.2.2" to 4000, parseRelay("[::ffff:10.0.2.2]:4000"))
    }

    @Test
    fun extraColonsAndOddPortsAreRefused() {
        // Two colons used to be taken for a bare IPv6 address; brackets used to take any text.
        for (bad in listOf("10.0.2.2:51820:1", "relay.example.com:4000:", "[10.0.2.2]:4000", "[::1]:+80",
                "10.0.2.2:+80", "fe80::1:zz")) {
            assertNull("\"$bad\" should be refused", parseRelay(bad))
        }
    }

    @Test
    fun badInputIsRefused() {
        for (bad in listOf("", ":51820", "10.0.2.2:", "10.0.2.2:abc", "10.0.2.2:0", "10.0.2.2:65536",
                "[::1", "[::1]4000", "[::1]:", "[]:4000")) {
            assertNull("\"$bad\" should be refused", parseRelay(bad))
        }
    }
}
