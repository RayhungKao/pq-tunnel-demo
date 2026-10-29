package io.github.rayhungkao.pqtunnel

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.os.Bundle
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts

class MainActivity : ComponentActivity() {

    private lateinit var relay: EditText
    private lateinit var status: TextView

    private val vpnConsent = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        if (it.resultCode == Activity.RESULT_OK) startTunnel() else status.text = "VPN permission denied"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val pad = (16 * resources.displayMetrics.density).toInt()
        relay = EditText(this).apply {
            // 10.0.2.2 is the host machine as seen from the Android emulator.
            setText("10.0.2.2:51820")
            hint = "relay host:port"
            contentDescription = "Relay address"
        }
        status = TextView(this).apply { textSize = 16f }
        val connect = Button(this).apply {
            text = "Connect"
            setOnClickListener {
                // prepare() returns null once the user has already allowed this app to be a VPN.
                VpnService.prepare(this@MainActivity)?.let(vpnConsent::launch) ?: startTunnel()
            }
        }
        val disconnect = Button(this).apply {
            text = "Disconnect"
            setOnClickListener {
                startService(Intent(this@MainActivity, TunnelService::class.java).setAction(TunnelService.ACTION_STOP))
            }
        }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, pad)
            addView(relay)
            addView(connect)
            addView(disconnect)
            addView(status)
        })
    }

    override fun onStart() {
        super.onStart()
        TunnelService.listener = { runOnUiThread { render(it) } }
        render(TunnelService.lastStatus)
    }

    override fun onStop() {
        TunnelService.listener = null
        super.onStop()
    }

    private fun render(text: String) {
        status.text = "$text\nSent ${TunnelService.packetsSent} · received ${TunnelService.packetsReceived} packets\n" +
            "Try: ping 10.66.0.1"
    }

    private fun startTunnel() {
        startService(Intent(this, TunnelService::class.java).putExtra(TunnelService.EXTRA_RELAY, relay.text.toString().trim()))
    }
}
