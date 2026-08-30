package zip.iptables.pandar.android.domain.model

sealed interface PrinterControlIntent {
    data object Pause : PrinterControlIntent
    data object Resume : PrinterControlIntent
    data object Stop : PrinterControlIntent
    data object ToggleLight : PrinterControlIntent
    data class SetChamberLight(val on: Boolean) : PrinterControlIntent
    data class Home(val axes: List<PrinterAxis> = emptyList()) : PrinterControlIntent
    data class MoveAxes(
        val movements: List<PrinterAxisMovement>,
        val feedrateMmPerMin: Int? = null,
    ) : PrinterControlIntent
    data class SetHotendTemperature(
        val temperatureCelsius: Int,
        val wait: Boolean,
        val extruderId: Int? = null,
    ) : PrinterControlIntent
    data class SetBedTemperature(
        val temperatureCelsius: Int,
        val wait: Boolean,
    ) : PrinterControlIntent
    data class SetChamberTemperature(
        val temperatureCelsius: Int,
        val wait: Boolean,
    ) : PrinterControlIntent
    data class AmsRereadRfid(val amsId: Int, val slotId: Int) : PrinterControlIntent
    data class AmsLoadFilament(
        val amsId: Int,
        val slotId: Int,
        val globalTrayId: Int? = null,
        val externalId: String? = null,
        val extruderId: Int? = null,
    ) : PrinterControlIntent
    data class AmsUnloadFilament(
        val amsId: Int,
        val slotId: Int,
        val globalTrayId: Int? = null,
        val externalId: String? = null,
        val extruderId: Int? = null,
    ) : PrinterControlIntent
}

enum class PrinterAxis { X, Y, Z }

data class PrinterAxisMovement(
    val axis: PrinterAxis,
    val deltaMm: Double,
)

fun moveAxisIntent(axis: PrinterAxis, deltaMm: Double) = PrinterControlIntent.MoveAxes(
    movements = listOf(PrinterAxisMovement(axis = axis, deltaMm = deltaMm)),
    feedrateMmPerMin = when (axis) {
        PrinterAxis.X, PrinterAxis.Y -> 3000
        PrinterAxis.Z -> 900
    },
)
