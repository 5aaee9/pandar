package zip.iptables.pandar.android.data.remote.dto

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
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
    nozzleTemperatures = nozzleTemperatures.map { it.toDomain() },
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
    id, tenantId, agentId, printerId, kind, status, payloadJson, error, resultJson, createdAt, updatedAt,
)

fun JobDto.toDomain(): Job = Job(
    id = id,
    printerId = printerId,
    agentId = agentId,
    artifactId = artifactId,
    commandId = commandId,
    status = status,
    error = error,
    createdAt = createdAt,
    updatedAt = updatedAt,
    print = JobPrint(
        print.status,
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

fun PrinterEventCommandDto.toDomain(): Command = Command(
    id, tenantId, agentId, printerId, kind, status, payloadJson, error, resultJson, createdAt, updatedAt,
)

fun PrinterMaterialsDto.toDomain(): Materials {
    return Materials(
        amsUnits = amsUnits.asArray().map { unit ->
            val obj = unit as? JsonObject
            AmsUnit(
                unitId = obj?.stringField("unit_id"),
                unitKind = obj?.stringField("unit_kind"),
                humidity = obj?.flexibleString("humidity") ?: obj?.flexibleString("humidity_level"),
                trays = (obj?.get("trays") as? JsonArray ?: JsonArray(emptyList()))
                    .mapNotNull { it as? JsonObject }
                    .map { tray ->
                        AmsTray(
                            trayId = tray.stringField("tray_id"),
                            type = tray.stringField("type"),
                            color = tray.stringField("color"),
                            name = tray.stringField("name"),
                            remainingEstimate = tray.flexibleString("remaining_estimate"),
                            kValue = tray.flexibleString("k_value"),
                            globalTrayId = tray.intField("global_tray_id"),
                            exists = tray.boolField("exists"),
                        )
                    },
            )
        },
        externalSpools = externalSpools.asArray().mapNotNull { it as? JsonObject }.map { spool ->
            ExternalSpool(
                externalId = spool.stringField("external_id"),
                trayId = spool.stringField("tray_id"),
                type = spool.stringField("type"),
                color = spool.stringField("color"),
                name = spool.stringField("name"),
                remainingEstimate = spool.flexibleString("remaining_estimate"),
                kValue = spool.flexibleString("k_value"),
                globalTrayId = spool.intField("global_tray_id"),
                exists = spool.boolField("exists"),
            )
        },
        activeTray = (activeTray as? JsonObject)?.let {
            ActiveTray(
                kind = it.stringField("kind"),
                amsId = it.stringField("ams_id"),
                trayId = it.stringField("tray_id"),
                globalTrayId = it.intField("global_tray_id"),
                externalId = it.stringField("external_id"),
            )
        },
        observedAt = observedAt,
    )
}

private fun JsonElement?.asArray(): List<JsonElement> =
    (this as? JsonArray) ?: JsonArray(emptyList())

private fun JsonObject.stringField(key: String): String? =
    (get(key) as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull
        ?: (get(key) as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotEmpty() }

private fun JsonObject.flexibleString(key: String): String? {
    val primitive = get(key) as? JsonPrimitive ?: return null
    if (primitive.isString) return primitive.contentOrNull?.takeIf { it.isNotEmpty() }
    return primitive.contentOrNull?.takeIf { it.isNotEmpty() }
}

private fun JsonObject.intField(key: String): Int? =
    (get(key) as? JsonPrimitive)?.intOrNull

private fun JsonObject.boolField(key: String): Boolean? =
    (get(key) as? JsonPrimitive)?.contentOrNull?.let { it == "true" }
