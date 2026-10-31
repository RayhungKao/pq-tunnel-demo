package io.github.rayhungkao.pqtunnel

import android.content.Intent
import android.net.VpnService
import android.os.ParcelFileDescriptor
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import android.system.StructPollfd
import android.util.Base64
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

/**
 * Routes 10.66.0.0/24 through an encrypted UDP tunnel to the relay. Only that subnet goes through
 * the tunnel, so the rest of the phone's traffic is untouched.
 */
class TunnelService : VpnService() {

    /**
     * One connection attempt. [stop] may run before the attempt has opened its socket or
     * interface; the attempt checks [stopped] after opening each, so the stop is never lost.
     */
    private class Run(val id: Int) {
        @Volatile var stopped = false
        @Volatile var socket: DatagramSocket? = null
        @Volatile var tun: ParcelFileDescriptor? = null

        fun stop() {
            stopped = true
            socket?.close()
            tun?.close()
        }
    }

    @Volatile private var current: Run? = null
    private var worker: Thread? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        stopTunnel()
        if (intent?.action == ACTION_STOP) {
            // With the id, Android keeps the service if a newer Connect has already arrived;
            // without it, onDestroy() would stop that new connection too.
            stopSelf(startId)
            return START_NOT_STICKY
        }
        val relay = intent?.getStringExtra(EXTRA_RELAY) ?: return START_NOT_STICKY
        // A newer run makes the old one's late status messages stale, so they are dropped.
        val run = Run(generation.incrementAndGet()).also { current = it }
        worker = thread(name = "tunnel") { runTunnel(relay, run) }
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
        // Closing the socket and interface ends both packet loops within POLL_MS.
        current?.stop()
        worker?.join(1000)
        worker = null
    }

    private fun runTunnel(relay: String, run: Run) {
        val (host, port) = parseRelay(relay) ?: return report(run, "Bad relay address: $relay")
        val session = try {
            Native.connect(Base64.decode(assets.open("server.pub").readBytes(), Base64.DEFAULT))
        } catch (e: Exception) {
            0L // missing asset or broken base64
        }
        if (session == 0L) {
            report(run, "No valid server key in the APK")
            return
        }
        var receiver: Thread? = null
        // Shown instead of "Disconnected" when the run ends, so the user sees why it stopped.
        var error: String? = null
        try {
            // Look the name up before opening anything: it can take seconds, and a stop during it
            // must end the run here, not after establish() has brought up tun0.
            val address = InetAddress.getByName(host)
            if (run.stopped) return
            // "::" and "0.0.0.0" name no host; sending to them would wait for an answer forever.
            if (address.isAnyLocalAddress) throw IOException("$host is not a relay address")
            val sock = DatagramSocket().also { run.socket = it }
            if (run.stopped) return
            // Keep the tunnel's own packets outside the tunnel, or they would loop back into it.
            protect(sock)
            sock.connect(InetSocketAddress(address, port))
            // Last check before tun0 comes up. A stop after this point cannot interrupt establish();
            // the check right after it closes tun0 again at once.
            if (run.stopped) return

            val tunFd = Builder()
                .setSession("PQ Tunnel Demo")
                .addAddress("10.66.0.2", 32)
                .addRoute("10.66.0.0", 24)
                // Leaves room for the 33-byte tunnel header inside a typical 1500-byte path.
                .setMtu(1280)
                .establish() ?: throw IOException("VPN permission was revoked")
            run.tun = tunFd
            if (run.stopped) return
            // "Connected" waits for the relay's ACK: before it, nothing proves the relay is there.
            report(run, "Waiting for the relay at $host:$port")

            val init = Native.initPacket(session) ?: throw IOException("Native session is gone")
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
                var connected = false
                try {
                    while (true) {
                        sock.receive(packet)
                        val ip = Native.open(session, buf.copyOf(packet.length))
                        if (ip != null) {
                            toPhone.write(ip)
                            packetsReceived++ // only this thread writes it
                        } else if (!connected && Native.ready(session)) {
                            connected = true
                            report(run, "Connected to $host:$port (X25519 + ML-KEM-768)")
                        }
                    }
                } catch (_: IOException) {
                    // stopTunnel() closed the socket.
                }
            }

            val fromPhone = FileInputStream(tunFd.fileDescriptor)
            val buf = ByteArray(65535)
            val readable = StructPollfd().apply {
                fd = tunFd.fileDescriptor
                events = OsConstants.POLLIN.toShort()
            }
            try {
                // The tun fd from establish() is non-blocking (O_NONBLOCK, read from fdinfo), so a
                // bare read loop spins on empty reads and keeps a core busy. Wait in poll instead,
                // with a timeout, and stop once stopTunnel() has closed the socket.
                while (!sock.isClosed) {
                    if (!pollQuietly(readable)) continue
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
            // After a stop, failures are expected (closed socket, interrupted lookup) and not news.
            if (!run.stopped) error = "Error: ${e.message ?: e.javaClass.simpleName}"
        } finally {
            run.socket?.close()
            run.tun?.close()
            // Both packet loops have stopped; a late call with the freed handle would get null.
            receiver?.join()
            Native.free(session)
            report(run, error ?: "Disconnected")
        }
    }

    /** True when [pollfd] has something to read; false after [POLL_MS] or an interrupted wait. */
    private fun pollQuietly(pollfd: StructPollfd): Boolean = try {
        Os.poll(arrayOf(pollfd), POLL_MS) > 0
    } catch (e: ErrnoException) {
        if (e.errno != OsConstants.EINTR) throw IOException(e)
        false
    }

    private fun sendQuietly(sock: DatagramSocket, bytes: ByteArray): Boolean = try {
        sock.send(DatagramPacket(bytes, bytes.size))
        true
    } catch (_: IOException) {
        false
    }

    private fun report(run: Run, status: String) {
        if (run.id != generation.get()) return
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
        private const val POLL_MS = 500
        private val generation = AtomicInteger()
    }
}
