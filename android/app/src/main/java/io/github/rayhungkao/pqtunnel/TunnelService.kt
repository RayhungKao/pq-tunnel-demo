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
        val (host, port) = relay.split(":").let { it[0] to it.getOrElse(1) { "51820" }.toInt() }
        val serverPublic = Base64.decode(assets.open("server.pub").readBytes(), Base64.DEFAULT)
        val session = Native.connect(serverPublic)
        if (session == 0L) {
            report("Bad server key in the APK")
            return
        }
        try {
            val udp = DatagramSocket().also { socket = it }
            // Keep the tunnel's own packets outside the tunnel, or they would loop back into it.
            protect(udp)
            udp.connect(InetSocketAddress(host, port))

            val iface = Builder()
                .setSession("PQ Tunnel Demo")
                .addAddress("10.66.0.2", 32)
                .addRoute("10.66.0.0", 24)
                // Leaves room for the 33-byte tunnel header inside a typical 1500-byte path.
                .setMtu(1280)
                .establish() ?: throw IOException("VPN permission was revoked")
            tun = iface

            val init = Native.initPacket(session)
            udp.send(DatagramPacket(init, init.size))
            report("Connected to $host:$port (X25519 + ML-KEM-768)")

            val toPhone = FileOutputStream(iface.fileDescriptor)
            val receiver = thread(name = "tunnel-rx") {
                val buf = ByteArray(65535)
                val packet = DatagramPacket(buf, buf.size)
                try {
                    while (true) {
                        udp.receive(packet)
                        Native.open(session, buf.copyOf(packet.length))?.let {
                            toPhone.write(it)
                            count(received = 1)
                        }
                    }
                } catch (_: IOException) {
                }
            }

            val fromPhone = FileInputStream(iface.fileDescriptor)
            val buf = ByteArray(65535)
            try {
                while (true) {
                    val n = fromPhone.read(buf)
                    if (n <= 0) continue
                    val sealed = Native.seal(session, buf.copyOf(n))
                    udp.send(DatagramPacket(sealed, sealed.size))
                    count(sent = 1)
                }
            } catch (_: IOException) {
            }
            udp.close()
            receiver.join()
        } catch (e: Exception) {
            report("Error: ${e.message}")
        } finally {
            socket?.close()
            tun?.close()
            // Both loops have stopped, so nothing can use the handle after this.
            Native.free(session)
            report("Disconnected")
        }
    }

    private fun count(sent: Int = 0, received: Int = 0) {
        packetsSent += sent
        packetsReceived += received
        listener?.invoke(lastStatus)
    }

    private fun report(status: String) {
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
    }
}
