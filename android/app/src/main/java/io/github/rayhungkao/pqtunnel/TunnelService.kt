package io.github.rayhungkao.pqtunnel

import android.content.Intent
import android.net.VpnService
import android.os.ParcelFileDescriptor
import android.util.Base64
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

/**
 * Routes 10.66.0.0/24 through an encrypted UDP tunnel to the relay. Only that subnet goes through
 * the tunnel, so the rest of the phone's traffic is untouched.
 */
class TunnelService : VpnService() {

    @Volatile private var tun: ParcelFileDescriptor? = null
    @Volatile private var socket: DatagramSocket? = null
    private var worker: Thread? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        stopTunnel()
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        val relay = intent?.getStringExtra(EXTRA_RELAY) ?: return START_NOT_STICKY
        worker = thread(name = "tunnel") { runTunnel(relay) }
        return START_STICKY
    }

    override fun onRevoke() {
        // The user turned the VPN off in system settings.
        stopTunnel()
        super.onRevoke()
    }

    override fun onDestroy() {
        stopTunnel()
        super.onDestroy()
    }

    private fun stopTunnel() {
        // Closing both ends makes the blocking reads below throw, which ends the worker.
        socket?.close()
        tun?.close()
        worker?.join(1000)
        worker = null
    }

    private fun runTunnel(relay: String) {
        // A newer run makes this one's late status messages stale, so they are dropped.
        val run = generation.incrementAndGet()
        val parts = relay.split(":")
        val host = parts[0]
        val port = parts.getOrElse(1) { "51820" }.toIntOrNull()
        if (host.isEmpty() || port == null || port !in 1..65535) {
            report(run, "Bad relay address: $relay")
            return
        }
        val session = try {
            Native.connect(Base64.decode(assets.open("server.pub").readBytes(), Base64.DEFAULT))
        } catch (e: Exception) {
            0L // missing asset or broken base64
        }
        if (session == 0L) {
            report(run, "No valid server key in the APK")
            return
        }
        // This run's own socket and interface. Cleanup uses these, never the fields, which a newer
        // run may already have replaced.
        var udp: DatagramSocket? = null
        var iface: ParcelFileDescriptor? = null
        var receiver: Thread? = null
        try {
            val sock = DatagramSocket().also { udp = it; socket = it }
            // Keep the tunnel's own packets outside the tunnel, or they would loop back into it.
            protect(sock)
            sock.connect(InetSocketAddress(host, port))

            val tunFd = Builder()
                .setSession("PQ Tunnel Demo")
                .addAddress("10.66.0.2", 32)
                .addRoute("10.66.0.0", 24)
                // Leaves room for the 33-byte tunnel header inside a typical 1500-byte path.
                .setMtu(1280)
                .establish() ?: throw IOException("VPN permission was revoked")
            iface = tunFd
            tun = tunFd
            report(run, "Connected to $host:$port (X25519 + ML-KEM-768)")

            val init = Native.initPacket(session)
            // UDP can drop the INIT, and a restarted relay forgets every session, so keep resending
            // it. A live relay answers with its original ACK; a restarted one sends a new ACK, and
            // the core switches to that relay's fresh keys.
            thread(name = "tunnel-init", isDaemon = true) {
                while (!sock.isClosed) {
                    sendQuietly(sock, init)
                    Thread.sleep(INIT_RESEND_MS)
                }
            }

            val toPhone = FileOutputStream(tunFd.fileDescriptor)
            receiver = thread(name = "tunnel-rx") {
                val buf = ByteArray(65535)
                val packet = DatagramPacket(buf, buf.size)
                try {
                    while (true) {
                        sock.receive(packet)
                        Native.open(session, buf.copyOf(packet.length))?.let {
                            toPhone.write(it)
                            packetsReceived++ // only this thread writes it
                        }
                    }
                } catch (_: IOException) {
                    // stopTunnel() closed the socket.
                }
            }

            val fromPhone = FileInputStream(tunFd.fileDescriptor)
            val buf = ByteArray(65535)
            try {
                while (true) {
                    val n = fromPhone.read(buf)
                    if (n < 0) break
                    if (n == 0) continue
                    // A send fails for a moment while the phone switches networks: drop that one
                    // packet and keep the tunnel.
                    val sealed = Native.seal(session, buf.copyOf(n)) ?: continue // no ACK yet
                    if (sendQuietly(sock, sealed)) packetsSent++
                }
            } catch (_: IOException) {
                // stopTunnel() closed the interface.
            }
        } catch (e: Exception) {
            report(run, "Error: ${e.message}")
        } finally {
            udp?.close()
            iface?.close()
            // Both packet loops must have stopped before the handle is freed.
            receiver?.join()
            Native.free(session)
            report(run, "Disconnected")
        }
    }

    private fun sendQuietly(sock: DatagramSocket, bytes: ByteArray): Boolean = try {
        sock.send(DatagramPacket(bytes, bytes.size))
        true
    } catch (_: IOException) {
        false
    }

    private fun report(run: Int, status: String) {
        if (run != generation.get()) return
        lastStatus = status
        listener?.invoke(status)
    }

    companion object {
        const val ACTION_STOP = "stop"
        const val EXTRA_RELAY = "relay"

        // shortcut: process-wide state for one screen; a bound service or ViewModel would replace it in a real app.
        @Volatile var lastStatus = "Disconnected"
        @Volatile var packetsSent = 0
        @Volatile var packetsReceived = 0
        @Volatile var listener: ((String) -> Unit)? = null

        private const val INIT_RESEND_MS = 10_000L
        private val generation = AtomicInteger()
    }
}
