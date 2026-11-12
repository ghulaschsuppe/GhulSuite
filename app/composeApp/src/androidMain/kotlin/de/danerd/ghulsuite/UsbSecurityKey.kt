// SPDX-License-Identifier: AGPL-3.0-or-later
package de.danerd.ghulsuite

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.hardware.usb.UsbConstants
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbDeviceConnection
import android.hardware.usb.UsbEndpoint
import android.hardware.usb.UsbInterface
import android.hardware.usb.UsbManager
import android.os.Build
import de.danerd.ghulsuite.core.HidTransport
import de.danerd.ghulsuite.core.TransportException
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine

/** Vendors of common FIDO security keys; tried first when several HID devices are attached. */
private val KNOWN_FIDO_VENDORS = setOf(
    0x1050, // Yubico
    0x20a0, // Nitrokey
    0x096e, // Feitian
    0x1209, // SoloKeys (pid.codes)
    0x0483, // SoloKeys v1 / STM
    0x349e, // Token2
    0x311f, // TrustKey
    0x2ccf, // HyperFIDO
    0x1ea8, // Thetis
    0x18d1, // Google Titan
)

/** Discovers FIDO security keys on USB and opens them as raw HID transports. */
class UsbSecurityKeys(private val context: Context) {
    private val usb: UsbManager = context.getSystemService(UsbManager::class.java)

    /** USB devices with at least one HID interface, known FIDO vendors first. */
    fun candidates(): List<UsbDevice> =
        usb.deviceList.values
            .filter { d -> (0 until d.interfaceCount).any { d.getInterface(it).interfaceClass == UsbConstants.USB_CLASS_HID } }
            .sortedByDescending { it.vendorId in KNOWN_FIDO_VENDORS }

    suspend fun ensurePermission(device: UsbDevice): Boolean {
        if (usb.hasPermission(device)) return true
        val action = "${context.packageName}.USB_PERMISSION"
        return suspendCancellableCoroutine { cont ->
            val receiver = object : BroadcastReceiver() {
                override fun onReceive(c: Context, intent: Intent) {
                    if (intent.action != action) return
                    runCatching { context.unregisterReceiver(this) }
                    val granted = intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)
                    if (cont.isActive) cont.resume(granted)
                }
            }
            val filter = IntentFilter(action)
            if (Build.VERSION.SDK_INT >= 33) {
                context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
            } else {
                @Suppress("UnspecifiedRegisterReceiverFlag")
                context.registerReceiver(receiver, filter)
            }
            cont.invokeOnCancellation { runCatching { context.unregisterReceiver(receiver) } }
            val intent = Intent(action).setPackage(context.packageName)
            val pending = PendingIntent.getBroadcast(
                context, 0, intent, PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            usb.requestPermission(device, pending)
        }
    }

    /** Opens the FIDO HID interface of [device] or throws [KeyException]. */
    fun open(device: UsbDevice): UsbHidTransport {
        val conn = usb.openDevice(device)
            ?: throw KeyException(KeyErrorKind.TRANSPORT, "openDevice failed for ${describe(device)}")
        for (i in 0 until device.interfaceCount) {
            val iface = device.getInterface(i)
            if (iface.interfaceClass != UsbConstants.USB_CLASS_HID) continue
            var inEp: UsbEndpoint? = null
            var outEp: UsbEndpoint? = null
            for (e in 0 until iface.endpointCount) {
                val ep = iface.getEndpoint(e)
                if (ep.type != UsbConstants.USB_ENDPOINT_XFER_INT) continue
                if (ep.direction == UsbConstants.USB_DIR_IN) inEp = ep else outEp = ep
            }
            if (inEp == null || outEp == null) continue
            if (!conn.claimInterface(iface, true)) continue
            if (isFidoInterface(conn, iface)) {
                return UsbHidTransport(usb, device, conn, iface, inEp, outEp)
            }
            conn.releaseInterface(iface)
        }
        conn.close()
        throw KeyException(KeyErrorKind.NOT_A_FIDO_KEY, describe(device))
    }

    /** Checks the HID report descriptor for the FIDO usage page (0xF1D0). */
    private fun isFidoInterface(conn: UsbDeviceConnection, iface: UsbInterface): Boolean {
        val buf = ByteArray(512)
        val n = conn.controlTransfer(0x81, 0x06, 0x2200, iface.id, buf, buf.size, 1000)
        if (n < 0) return true // descriptor not readable; CTAPHID INIT will tell
        for (k in 0 until n - 2) {
            if (buf[k] == 0x06.toByte() && buf[k + 1] == 0xD0.toByte() && buf[k + 2] == 0xF1.toByte()) return true
        }
        return false
    }

    companion object {
        fun describe(d: UsbDevice): String {
            val name = listOfNotNull(d.manufacturerName, d.productName).joinToString(" ").ifBlank { d.deviceName }
            return "$name (%04x:%04x)".format(d.vendorId, d.productId)
        }
    }
}

/** Raw 64-byte HID reports over USB interrupt endpoints. */
class UsbHidTransport(
    private val usb: UsbManager,
    val device: UsbDevice,
    private val conn: UsbDeviceConnection,
    private val iface: UsbInterface,
    private val inEp: UsbEndpoint,
    private val outEp: UsbEndpoint,
) : HidTransport, AutoCloseable {

    override fun reportSize(): UInt = outEp.maxPacketSize.toUInt()

    override fun writeReport(report: ByteArray) {
        val n = conn.bulkTransfer(outEp, report, report.size, 2_000)
        if (n != report.size) throw TransportException.Io("USB write failed ($n)")
    }

    override fun readReport(timeoutMs: UInt): ByteArray? {
        val buf = ByteArray(inEp.maxPacketSize)
        val n = conn.bulkTransfer(inEp, buf, buf.size, timeoutMs.toInt())
        if (n > 0) return buf.copyOf(n)
        if (usb.deviceList.values.none { it.deviceName == device.deviceName }) {
            throw TransportException.Io("key disconnected")
        }
        return null
    }

    override fun close() {
        runCatching { conn.releaseInterface(iface) }
        conn.close()
    }
}

enum class KeyErrorKind { TRANSPORT, NOT_A_FIDO_KEY }

/** USB-level problem while opening a key. */
class KeyException(val kind: KeyErrorKind, val detail: String) : Exception("$kind: $detail")
