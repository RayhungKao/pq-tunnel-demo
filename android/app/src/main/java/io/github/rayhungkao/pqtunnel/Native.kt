package io.github.rayhungkao.pqtunnel

/** The Rust tunnel core (core/src/android.rs). A session is an opaque handle; call [free] once. */
object Native {
    init {
        System.loadLibrary("tunnel_core")
    }

    /** Returns 0 when [serverPublic] is not a valid server key. */
    @JvmStatic external fun connect(serverPublic: ByteArray): Long
    @JvmStatic external fun initPacket(session: Long): ByteArray
    /** Returns null until the relay's ACK has arrived; drop the packet then. */
    @JvmStatic external fun seal(session: Long, ipPacket: ByteArray): ByteArray?

    /**
     * Returns the IP packet inside a DATA packet, or null for an ACK (handled inside) and for
     * packets that fail authentication or the replay check.
     */
    @JvmStatic external fun open(session: Long, packet: ByteArray): ByteArray?
    /** True once the relay's ACK has arrived and data can flow. */
    @JvmStatic external fun ready(session: Long): Boolean
    @JvmStatic external fun free(session: Long)
}
