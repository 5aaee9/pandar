package zip.iptables.pandar.android.data.remote

import okhttp3.HttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.logging.HttpLoggingInterceptor
import retrofit2.Retrofit
import retrofit2.converter.kotlinx.serialization.asConverterFactory
import java.util.concurrent.TimeUnit
import zip.iptables.pandar.android.BuildConfig

object ApiModule {

    fun okHttp(
        tokenProvider: TokenProvider,
        tokenRefresher: (suspend () -> Boolean)? = null,
        clearTokens: () -> Unit = {},
        logger: zip.iptables.pandar.android.core.util.Logger? = null,
    ): OkHttpClient {
        val builder = OkHttpClient.Builder()
            .connectTimeout(30, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .writeTimeout(30, TimeUnit.SECONDS)
            .addInterceptor(BearerAuthInterceptor(tokenProvider))
        if (tokenRefresher != null && logger != null) {
            builder.authenticator(TokenRefreshAuthenticator(tokenProvider, tokenRefresher, clearTokens, logger))
        }
        // Body-level logging in debug builds only.
        if (BuildConfig.DEBUG) {
            builder.addInterceptor(HttpLoggingInterceptor().apply {
                level = HttpLoggingInterceptor.Level.BASIC
            })
        }
        return builder.build()
    }

    fun webSocketHttp(): OkHttpClient {
        val builder = OkHttpClient.Builder()
            .connectTimeout(30, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .writeTimeout(30, TimeUnit.SECONDS)
        if (BuildConfig.DEBUG) {
            builder.addInterceptor(HttpLoggingInterceptor().apply {
                level = HttpLoggingInterceptor.Level.BASIC
            })
        }
        return builder.build()
    }

    fun retrofit(baseUrl: HttpUrl, client: OkHttpClient): Retrofit {
        val contentType = "application/json".toMediaType()
        return Retrofit.Builder()
            .baseUrl(baseUrl)
            .client(client)
            .addConverterFactory(appJson.asConverterFactory(contentType))
            .build()
    }

    fun pandarApi(baseUrl: HttpUrl, client: OkHttpClient): PandarApi =
        retrofit(baseUrl, client).create(PandarApi::class.java)
}
