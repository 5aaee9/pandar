package zip.iptables.pandar.android.data.remote.dto

import zip.iptables.pandar.android.domain.model.PrinterAxis
import zip.iptables.pandar.android.domain.model.PrinterControlIntent

internal fun PrinterControlIntent.toRequest(): PrinterControlRequest = when (this) {
    PrinterControlIntent.Pause -> PrinterControlRequest(action = "pause")
    PrinterControlIntent.Resume -> PrinterControlRequest(action = "resume")
    PrinterControlIntent.Stop -> PrinterControlRequest(action = "stop")
    PrinterControlIntent.ToggleLight -> PrinterControlRequest(action = "toggle_light")
    is PrinterControlIntent.SetChamberLight -> PrinterControlRequest(
        action = "set_chamber_light",
        lightOn = on,
    )
    is PrinterControlIntent.Home -> PrinterControlRequest(
        action = "home",
        axes = axes.map(PrinterAxis::toRequest),
    )
    is PrinterControlIntent.MoveAxes -> PrinterControlRequest(
        action = "move_axes",
        movements = movements.map { movement ->
            PrinterAxisMovementRequest(
                axis = movement.axis.toRequest(),
                deltaMm = movement.deltaMm,
            )
        },
        feedrateMmPerMin = feedrateMmPerMin,
    )
    is PrinterControlIntent.SetHotendTemperature -> PrinterControlRequest(
        action = "set_hotend_temperature",
        temperatureCelsius = temperatureCelsius,
        wait = wait,
        extruderId = extruderId,
    )
    is PrinterControlIntent.SetBedTemperature -> PrinterControlRequest(
        action = "set_bed_temperature",
        temperatureCelsius = temperatureCelsius,
        wait = wait,
    )
    is PrinterControlIntent.SetChamberTemperature -> PrinterControlRequest(
        action = "set_chamber_temperature",
        temperatureCelsius = temperatureCelsius,
        wait = wait,
    )
    is PrinterControlIntent.AmsRereadRfid -> PrinterControlRequest(
        action = "ams_reread_rfid",
        amsId = amsId,
        slotId = slotId,
    )
    is PrinterControlIntent.AmsLoadFilament -> PrinterControlRequest(
        action = "ams_load_filament",
        amsId = amsId,
        slotId = slotId,
        globalTrayId = globalTrayId,
        externalId = externalId,
        extruderId = extruderId,
    )
    is PrinterControlIntent.AmsUnloadFilament -> PrinterControlRequest(
        action = "ams_unload_filament",
        amsId = amsId,
        slotId = slotId,
        globalTrayId = globalTrayId,
        externalId = externalId,
        extruderId = extruderId,
    )
}

private fun PrinterAxis.toRequest() = when (this) {
    PrinterAxis.X -> PrinterAxisRequest.X
    PrinterAxis.Y -> PrinterAxisRequest.Y
    PrinterAxis.Z -> PrinterAxisRequest.Z
}
