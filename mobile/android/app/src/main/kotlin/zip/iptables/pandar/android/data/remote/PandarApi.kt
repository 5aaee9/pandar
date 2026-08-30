package zip.iptables.pandar.android.data.remote

import retrofit2.http.Body
import retrofit2.http.GET
import retrofit2.http.POST
import retrofit2.http.Path
import zip.iptables.pandar.android.data.remote.dto.AgentsListDto
import zip.iptables.pandar.android.data.remote.dto.CommandResponseDto
import zip.iptables.pandar.android.data.remote.dto.JobListDto
import zip.iptables.pandar.android.data.remote.dto.MobileTicketExchangeRequest
import zip.iptables.pandar.android.data.remote.dto.MobileTicketExchangeResponse
import zip.iptables.pandar.android.data.remote.dto.PrinterControlRequest
import zip.iptables.pandar.android.data.remote.dto.PrinterDto
import zip.iptables.pandar.android.data.remote.dto.PrinterListDto

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
    suspend fun control(
        @Path("tenant") tenant: String,
        @Path("printer") printer: String,
        @Body body: PrinterControlRequest,
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
