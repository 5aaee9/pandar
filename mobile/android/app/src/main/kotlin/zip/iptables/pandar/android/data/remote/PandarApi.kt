package zip.iptables.pandar.android.data.remote

import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path
import zip.iptables.pandar.android.data.remote.dto.AgentsListDto
import zip.iptables.pandar.android.data.remote.dto.AmsLoadFilamentRequest
import zip.iptables.pandar.android.data.remote.dto.AmsRereadRfidRequest
import zip.iptables.pandar.android.data.remote.dto.AmsUnloadFilamentRequest
import zip.iptables.pandar.android.data.remote.dto.CommandResponseDto
import zip.iptables.pandar.android.data.remote.dto.HomeRequest
import zip.iptables.pandar.android.data.remote.dto.JobListDto
import zip.iptables.pandar.android.data.remote.dto.MobileTicketExchangeRequest
import zip.iptables.pandar.android.data.remote.dto.MobileTicketExchangeResponse
import zip.iptables.pandar.android.data.remote.dto.MoveAxesRequest
import zip.iptables.pandar.android.data.remote.dto.PauseRequest
import zip.iptables.pandar.android.data.remote.dto.PrinterDto
import zip.iptables.pandar.android.data.remote.dto.PrinterListDto
import zip.iptables.pandar.android.data.remote.dto.ResumeRequest
import zip.iptables.pandar.android.data.remote.dto.SetBedTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.SetChamberLightRequest
import zip.iptables.pandar.android.data.remote.dto.SetChamberTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.SetHotendTemperatureRequest
import zip.iptables.pandar.android.data.remote.dto.StopRequest
import zip.iptables.pandar.android.data.remote.dto.ToggleLightRequest

interface PandarApi {

    @POST("api/v1/mobile/login-tickets/exchange")
    suspend fun exchangeMobileLoginTicket(@Body body: MobileTicketExchangeRequest): MobileTicketExchangeResponse

    @GET("api/v1/tenants/{tenant}/printers")
    suspend fun listPrinters(@Path("tenant") tenant: String): PrinterListDto

    @GET("api/v1/tenants/{tenant}/printers/{printer}")
    suspend fun getPrinter(@Path("tenant") tenant: String, @Path("printer") printer: String): PrinterDto

    @GET("api/v1/tenants/{tenant}/agents")
    suspend fun listAgents(@Path("tenant") tenant: String): AgentsListDto

    @GET("api/v1/tenants/{tenant}/jobs")
    suspend fun listJobs(@Path("tenant") tenant: String): JobListDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun pause(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: PauseRequest = PauseRequest(),
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun resume(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: ResumeRequest = ResumeRequest(),
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun stop(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: StopRequest = StopRequest(),
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun home(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: HomeRequest = HomeRequest(),
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun moveAxes(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: MoveAxesRequest,
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun toggleLight(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: ToggleLightRequest = ToggleLightRequest(),
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun setChamberLight(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: SetChamberLightRequest,
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun setHotendTemperature(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: SetHotendTemperatureRequest,
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun setBedTemperature(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: SetBedTemperatureRequest,
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun setChamberTemperature(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: SetChamberTemperatureRequest,
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun amsRereadRfid(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: AmsRereadRfidRequest,
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun amsLoadFilament(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: AmsLoadFilamentRequest,
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/printers/{printer}/controls")
    suspend fun amsUnloadFilament(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: AmsUnloadFilamentRequest,
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/jobs/{job}/retry-dispatch")
    suspend fun retryDispatch(
        @Path("tenant") tenant: String,
        @Path("job") job: String,
    ): CommandResponseDto

    @POST("api/v1/tenants/{tenant}/jobs/{job}/reprint")
    suspend fun reprint(
        @Path("tenant") tenant: String,
        @Path("job") job: String,
    ): CommandResponseDto
}
