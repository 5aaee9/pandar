package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import zip.iptables.pandar.android.domain.model.ActiveTray
import zip.iptables.pandar.android.domain.model.Agent
import zip.iptables.pandar.android.domain.model.AmsTray
import zip.iptables.pandar.android.domain.model.AmsUnit
import zip.iptables.pandar.android.domain.model.Command
import zip.iptables.pandar.android.domain.model.ExternalSpool
import zip.iptables.pandar.android.domain.model.Job
import zip.iptables.pandar.android.domain.model.JobArtifact
import zip.iptables.pandar.android.domain.model.JobPrint
import zip.iptables.pandar.android.domain.model.Materials
import zip.iptables.pandar.android.domain.model.Printer
import zip.iptables.pandar.android.domain.model.PrinterNozzleTemp

fun PrinterDto.toDomain(): Printer = Printer(
    id = id,
    tenantId = tenantId,
    agentId = agentId,
    serialNumber = serialNumber,
    name = name,
    model = model,
    status = status,
    lastSeenAt = lastSeenAt,
    createdAt = createdAt,
    nozzleTemperatures = nozzleTemperatures.orEmpty().map { it.toDomain() },
    activeNozzle = activeNozzle,
    bedTemperatureCelsius = bedTemperatureCelsius,
    bedTargetTemperatureCelsius = bedTargetTemperatureCelsius,
    chamberTemperatureCelsius = chamberTemperatureCelsius,
    chamberLightOn = chamberLightOn,
    materials = materials?.toDomain(),
)

private fun PrinterNozzleTempDto.toDomain(): PrinterNozzleTemp =
    PrinterNozzleTemp(label, currentCelsius, targetCelsius)

fun AgentDto.toDomain(): Agent = Agent(id, tenantId, name, status, createdAt)

fun CommandResponseDto.toDomain(): Command = Command(
    id,
    tenantId,
    agentId,
    printerId,
    kind,
    status.wireValue,
    payloadJson,
    error,
    resultJson,
    createdAt,
    updatedAt,
)

fun JobDto.toDomain(): Job = Job(
    id = id,
    printerId = printerId,
    agentId = agentId,
    artifactId = artifactId,
    commandId = commandId,
    status = status.wireValue,
    error = error,
    createdAt = createdAt,
    updatedAt = updatedAt,
    print = JobPrint(
        print.status.wireValue,
        print.progressPercent,
        print.remainingTimeMinutes,
        print.currentLayer,
        print.totalLayers,
        print.activeFile,
        print.error,
        print.startedAt,
        print.finishedAt,
        print.updatedAt,
    ),
    artifact = JobArtifact(
        artifact.id,
        artifact.filename,
        artifact.contentType,
        artifact.sizeBytes,
        artifact.createdAt,
    ),
)

fun PrinterMaterialsDto.toDomain(): Materials = Materials(
    amsUnits = amsUnits.map { unit ->
        AmsUnit(
            unitId = unit.unitId,
            unitKind = unit.unitKind,
            humidity = unit.humidity.flexibleString()
                ?: unit.humidityLevel.flexibleString(),
            trays = unit.trays.orEmpty().map { tray ->
                AmsTray(
                    trayId = tray.trayId,
                    type = tray.type,
                    color = tray.color,
                    name = tray.name,
                    remainingEstimate = tray.remainingEstimate.flexibleString(),
                    kValue = tray.kValue.flexibleString(),
                    globalTrayId = tray.globalTrayId,
                    exists = tray.exists,
                )
            },
        )
    },
    externalSpools = externalSpools.map { spool ->
        ExternalSpool(
            externalId = spool.externalId,
            trayId = spool.trayId,
            type = spool.type,
            color = spool.color,
            name = spool.name,
            remainingEstimate = spool.remainingEstimate.flexibleString(),
            kValue = spool.kValue.flexibleString(),
            globalTrayId = spool.globalTrayId,
            exists = spool.exists,
        )
    },
    activeTray = activeTray?.let {
        ActiveTray(
            kind = it.kind,
            amsId = it.amsId,
            trayId = it.trayId,
            globalTrayId = it.globalTrayId,
            externalId = it.externalId,
        )
    },
    observedAt = observedAt,
)

private fun JsonElement?.flexibleString(): String? =
    (this as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotEmpty() }
