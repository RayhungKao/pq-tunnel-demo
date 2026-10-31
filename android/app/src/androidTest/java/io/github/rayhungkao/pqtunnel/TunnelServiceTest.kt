package io.github.rayhungkao.pqtunnel

import android.content.Intent
import android.net.VpnService
import android.os.ParcelFileDescriptor
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress

/**
 * Drives the real TunnelService against a fake relay: a UDP socket on the device that receives the
 * INIT and can send stray datagrams back, but cannot answer with an ACK, because the test does not
 * have the server's private key. Each test's comment or name says which mistake it catches; each was
 * checked by reverting that fix and watching the test fail.
 */
@RunWith(AndroidJUnit4::class)
class TunnelServiceTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val context = instrumentation.targetContext
    private var liveAtStart = 0

    @Before
    fun allowVpn() {
        liveAtStart = Native.liveSessions()
        // The consent the user gives in the system dialog, granted from the shell instead. Reading
        // the output waits for the command to finish.
        val out = instrumentation.uiAutomation
            .executeShellCommand("appops set ${context.packageName} ACTIVATE_VPN allow")
        ParcelFileDescriptor.AutoCloseInputStream(out).use { it.readBytes() }
        // As MainActivity does before every Connect: with consent given, prepare() returns null and
        // makes this app the prepared VPN, without which establish() returns null.
        assertNull(VpnService.prepare(context))
    }

    @After
    fun disconnect() {
        stop()
        waitFor { !running() }
        // Every run frees its native session when it ends; a leak shows up here, in the test that
        // leaked. 15 s, because a stop cannot cut a name lookup short.
        val deadline = System.currentTimeMillis() + 15_000
        while (Native.liveSessions() > liveAtStart) {
            check(System.currentTimeMillis() < deadline) {
                "${Native.liveSessions() - liveAtStart} native sessions from this test were never freed"
            }
            Thread.sleep(50)
        }
    }

    @Test
    fun withoutAnAckItSendsInitAndKeepsWaiting() {
        fakeRelay("127.0.0.1").use { relay ->
            connect("127.0.0.1:${relay.localPort}")
            val init = receive(relay)
            assertEquals(1 + 32 + 1088, init.size)
            assertEquals(1, init[0].toInt())
            waitFor { TunnelService.lastStatus.startsWith("Waiting for the relay") }
            Thread.sleep(1000)
            assertTrue(TunnelService.lastStatus, TunnelService.lastStatus.startsWith("Waiting for the relay"))
        }
    }

    @Test
    fun aStrayDatagramIsNotTakenForTheAck() {
        fakeRelay("127.0.0.1").use { relay ->
            connect("127.0.0.1:${relay.localPort}")
            val buf = ByteArray(2048)
            val init = DatagramPacket(buf, buf.size)
            relay.receive(init)
            // Anything from the relay's address that is not a valid ACK.
            relay.send(DatagramPacket(ByteArray(40), 40, init.socketAddress))
            Thread.sleep(1000)
            assertTrue(TunnelService.lastStatus, TunnelService.lastStatus.startsWith("Waiting for the relay"))
        }
    }

    @Test
    fun anIdleTunnelUsesAlmostNoCpu() {
        fakeRelay("127.0.0.1").use { relay ->
            connect("127.0.0.1:${relay.localPort}")
            receive(relay)
            Thread.sleep(500)
            val before = senderCpuTicks()
            Thread.sleep(2000)
            val used = senderCpuTicks() - before
            // A loop that spins on the non-blocking tun fd uses about 200 ticks (one core) here.
            assertTrue("sender thread used $used ticks in 2 s", used < 20)
        }
    }

    @Test
    fun anUnspecifiedAddressIsRefused() {
        for ((relay, host) in listOf("[::]:51820" to "::", "0.0.0.0:51820" to "0.0.0.0")) {
            connect(relay)
            waitFor { TunnelService.lastStatus == "Error: $host is not a relay address" }
        }
    }

    @Test
    fun disconnectRightAfterConnectShowsDisconnectedNotTheLookupError() {
        connect("${unresolvableHost()}:51820")
        stop()
        // Wait for this run to end, however long the lookup takes; a fixed sleep could read the
        // status before the lookup fails.
        waitFor(timeoutMs = 15_000) { runEnded() }
        assertEquals("Disconnected", TunnelService.lastStatus)
    }

    @Test
    fun anUnresolvableHostShowsTheErrorAndKeepsIt() {
        connect("${unresolvableHost()}:51820")
        waitFor(timeoutMs = 15_000) { TunnelService.lastStatus.startsWith("Error:") }
        Thread.sleep(1500)
        assertTrue(TunnelService.lastStatus, TunnelService.lastStatus.startsWith("Error:"))
    }

    @Test
    fun anIpv6RelayAddressIsUsed() {
        fakeRelay("::1").use { relay ->
            connect("[::1]:${relay.localPort}")
            assertEquals(1, receive(relay)[0].toInt())
        }
    }

    /** Catches a stop that does not close the run (checked by making Run.stop() empty). */
    @Test
    fun disconnectEndsAnIdleTunnelQuickly() {
        fakeRelay("127.0.0.1").use { relay ->
            connect("127.0.0.1:${relay.localPort}")
            receive(relay)
            // Idle: no packet flows, so only the stop can end the packet loops.
            Thread.sleep(3000)
            stop()
            waitFor(timeoutMs = 2000) { TunnelService.lastStatus == "Disconnected" }
        }
    }

    @Test
    fun repeatedConnectAndDisconnectEndCleanly() {
        fakeRelay("127.0.0.1").use { relay ->
            repeat(10) { i ->
                connect("127.0.0.1:${relay.localPort}")
                waitFor { TunnelService.lastStatus.startsWith("Waiting for the relay") }
                // Every other round reconnects while connected, the rest disconnect first.
                if (i % 2 == 1) {
                    stop()
                    waitFor { TunnelService.lastStatus == "Disconnected" }
                }
            }
        }
        stop()
        waitFor { TunnelService.lastStatus == "Disconnected" }
    }

    /** User plus system CPU time, in clock ticks, of this process's threads named "tunnel". */
    private fun senderCpuTicks(): Long = File("/proc/self/task").listFiles()!!.sumOf { task ->
        // A thread can exit between the listing and the read (measured: 137 of 3000 reads failed
        // with ENOENT while threads came and went), so a vanished thread counts as 0.
        try {
            if (File(task, "comm").readText().trim() != "tunnel") return@sumOf 0L
            // Fields after the ")" that ends the thread name; utime and stime are fields 14 and 15.
            val fields = File(task, "stat").readText().substringAfterLast(')').trim().split(' ')
            fields[11].toLong() + fields[12].toLong()
        } catch (_: IOException) {
            0L
        }
    }

    /**
     * A fresh name under `.invalid` each time. Android caches failed lookups (measured on the
     * emulator: 124 ms for a new name, 0 ms when repeated), and a cached failure could return
     * before a stop sent right after Connect is processed.
     */
    private fun unresolvableHost() = "relay-${System.nanoTime()}.invalid"

    private fun connect(relay: String) {
        // The status outlives a run. Reset it, so the test only sees what this run reports.
        TunnelService.lastStatus = NOT_STARTED
        context.startService(Intent(context, TunnelService::class.java).putExtra(TunnelService.EXTRA_RELAY, relay))
    }

    private fun stop() {
        context.startService(Intent(context, TunnelService::class.java).setAction(TunnelService.ACTION_STOP))
    }

    private fun running() = TunnelService.lastStatus.let { it.startsWith("Waiting") || it.startsWith("Connected") }

    /** The run started by the last connect() has reported its final status and freed its session. */
    private fun runEnded() =
        TunnelService.lastStatus != NOT_STARTED && !running() && Native.liveSessions() <= liveAtStart

    private fun fakeRelay(address: String) =
        DatagramSocket(InetSocketAddress(InetAddress.getByName(address), 0)).apply { soTimeout = 5000 }

    private fun receive(socket: DatagramSocket): ByteArray {
        val buf = ByteArray(2048)
        val packet = DatagramPacket(buf, buf.size)
        socket.receive(packet)
        return buf.copyOf(packet.length)
    }

    private fun waitFor(timeoutMs: Long = 5000, condition: () -> Boolean) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (!condition()) {
            check(System.currentTimeMillis() < deadline) { "timed out; status is \"${TunnelService.lastStatus}\"" }
            Thread.sleep(50)
        }
    }

    private companion object {
        const val NOT_STARTED = "(test) not started"
    }
}
