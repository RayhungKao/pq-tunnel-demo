package io.github.rayhungkao.pqtunnel

/** The Rust tunnel core (core/src/android.rs). A session is an opaque handle; call [free] once. */
object Native {
    init {
        System.loadLibrary("tunnel_core")
    }

    /** Returns 0 when [serverPublic] is not a valid server key. */
    @JvmStatic external fun connect(serverPublic: ByteArray): Long
    @JvmStatic external fun initPacket(session: Long): ByteArray
    @JvmStatic external fun seal(session: Long, ipPacket: ByteArray): ByteArray

    /** Returns null for packets that fail authentication or the replay check. */
    @JvmStatic external fun open(session: Long, packet: ByteArray): ByteArray?
    @JvmStatic external fun free(session: Long)
}
